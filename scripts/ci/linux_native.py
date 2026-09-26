#!/usr/bin/env python3
"""A finite Linux native CI witness. No workspace/crash/fuzz or release claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import resource
import re
import shutil
import signal
import subprocess
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = Path(__file__).with_name("linux_ordinary_manifest.json")
SCHEMA = "anubis.linux-native-ordinary.v1"
CLAIM = "LINUX_NATIVE_ORDINARY_PASS"
PAYLOAD_COMPLETE = "PAYLOAD_COMPLETE_PENDING_TEARDOWN"
EXTERNAL = ["full-workspace-and-crash-tests", "full-soundness-matrix", "fuzz-and-stress",
            "tart-vz-seal", "require-metal", "omarchy-installation", "release-seal"]
CONTROL_ARGV = ["python3", "-B", "-m", "unittest", "discover", "-s", "scripts/ci",
                "-p", "test_linux_native.py"]
TRIPLES = {"x86_64": "x86_64-unknown-linux-gnu", "aarch64": "aarch64-unknown-linux-gnu"}


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def digest(path):
    with Path(path).open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def elf_identity(path, arch):
    # ELF64 e_ident and e_machine; constants match the Linux elf.h ABI.
    with Path(path).open("rb") as handle:
        header = handle.read(20)
    require(len(header) == 20 and header[:6] == b"\x7fELF\x02\x01"
            and int.from_bytes(header[18:20], "little") == {"x86_64": 62, "aarch64": 183}[arch],
            "executable is not native little-endian ELF64 for the requested architecture")
    return {"class": "ELF64", "byte_order": "little", "machine": arch}


def save(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


def output(argv, **kwargs):
    return subprocess.check_output(argv, text=True, timeout=30, **kwargs).strip()


def classified_tests(root, manifest):
    require(manifest.get("schema") == "anubis.linux-ordinary-classification.v1", "classification schema mismatch")
    require(manifest.get("targets"), "empty ordinary target roster")
    names = []
    for target in manifest["targets"]:
        source = root / target["source"]
        require(source.is_file() and not source.is_symlink(), "invalid classified source")
        require(digest(source) == target["sha256"],
                f"renew source classification before running {target['source']}")
        require(target["tests"], "empty ordinary test target")
        require(target.get("classification") == "ordinary-finite", "test is not ordinary-finite")
        require(re.findall(r"#\[test\]\s+fn ([A-Za-z0-9_]+)\(", source.read_text()) == target["tests"],
                "classified test names differ from reviewed source")
        names.extend(f"{target['package']}::{target['target']}::{name}" for name in target["tests"])
    require(len(names) == len(set(names)), "duplicate classified test")
    return names


def source_identity(root):
    require(not output(["git", "status", "--porcelain"], cwd=root), "source checkout is dirty")
    paths = subprocess.check_output(["git", "ls-files", "-z"], cwd=root).split(b"\0")
    files = {}
    for raw in filter(None, paths):
        name = os.fsdecode(raw)
        path = root / name
        files[name] = {"symlink": os.readlink(path)} if path.is_symlink() else {"sha256": digest(path)}
    return {"commit": output(["git", "rev-parse", "HEAD"], cwd=root),
            "tree": output(["git", "rev-parse", "HEAD^{tree}"], cwd=root), "files": files}


def validate_limits(values):
    require(values["memory.max"].isdigit() and 0 < int(values["memory.max"]) <= 6000000000,
            "missing/excessive memory cap")
    require(values["memory.swap.max"] == "0", "swap cap is not zero")
    cpu = values["cpu.max"].split()
    require(len(cpu) == 2 and all(x.isdigit() for x in cpu)
            and 0 < int(cpu[0]) <= 2 * int(cpu[1]), "missing/excessive CPU cap")
    require(values["pids.max"].isdigit() and 0 < int(values["pids.max"]) <= 512,
            "missing/excessive process cap")


def scope_info(unit, proc=Path("/proc/self/cgroup"), base=Path("/sys/fs/cgroup")):
    entries = [line[3:] for line in proc.read_text().splitlines() if line.startswith("0::")]
    require(len(entries) == 1, "cgroup v2 membership missing or ambiguous")
    relative = Path(entries[0])
    require(relative.is_absolute() and ".." not in relative.parts and relative.name == unit,
            "payload is outside its declared systemd service")
    scope = base / str(relative).lstrip("/")
    values = {name: (scope / name).read_text().strip() for name in
              ["memory.max", "memory.swap.max", "cpu.max", "pids.max"]}
    validate_limits(values)
    return scope, {"path": str(scope), "unit": unit, "limits": values}


def quiescent(scope):
    pids = {int(x) for p in scope.rglob("cgroup.procs") for x in p.read_text().split()}
    require(pids == {os.getpid()}, f"unexpected descendants remain in bounded service: {sorted(pids)}")


def test_result(path, name):
    events = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    require([(e.get("type"), e.get("event")) for e in events] == [
        ("suite", "started"), ("test", "started"), ("test", "ok"), ("suite", "ok")],
        f"unexpected libtest event stream: {name}")
    tests = [e for e in events if e.get("type") == "test"]
    require([(e.get("name"), e.get("event")) for e in tests] == [(name, "started"), (name, "ok")],
            f"missing, duplicate, failed, or unexpected test: {name}")
    suites = [e for e in events if e.get("type") == "suite"]
    require(len(suites) == 2 and suites[0].get("event") == "started"
            and suites[0].get("test_count") == 1 and suites[1].get("event") == "ok"
            and suites[1].get("passed") == 1
            and all(suites[1].get(k) == 0 for k in ["failed", "ignored", "measured"]),
            f"incomplete test suite: {name}")


def build_commands(manifest):
    builds = [("build-cli", ["cargo", "build", "--locked", "--release", "-p", "anubis",
                              "--message-format=json"])]
    for package in dict.fromkeys(t["package"] for t in manifest["targets"]):
        argv = ["cargo", "test", "--locked", "--release", "--no-run", "-p", package,
                "--message-format=json"]
        for target in manifest["targets"]:
            if target["package"] == package:
                argv.extend(["--test", target["target"]])
        builds.append((f"compile-{package}", argv))
    return builds


def cargo_executables(log, wanted, is_test):
    messages = [json.loads(line) for line in log.read_text().splitlines() if line.strip()]
    require([m.get("success") for m in messages if m.get("reason") == "build-finished"] == [True],
            "Cargo build-finished evidence missing or duplicated")
    found = {}
    for message in messages:
        if message.get("reason") != "compiler-artifact" or not message.get("executable"):
            continue
        name = message["target"]["name"]
        if name in wanted and message["target"]["kind"] == (["test"] if is_test else ["bin"]):
            require(message["profile"]["test"] is is_test and name not in found,
                    "wrong or duplicate Cargo executable artifact")
            found[name] = message["executable"]
    require(set(found) == set(wanted), "Cargo executable roster mismatch")
    return found


class Driver:
    def __init__(self, args):
        self.args = args
        self.out = args.out.resolve()
        self.out.mkdir(parents=True, exist_ok=True)
        self.receipt = {"schema": SCHEMA, "verdict": "FAIL", "external": EXTERNAL,
                        "expected_arch": args.arch, "expected_sha": args.sha,
                        "commands": [], "artifacts": {}, "error": "run incomplete"}
        self.scope = None
        self.env = None
        self.write()

    def write(self):
        save(self.out / "receipt.json", self.receipt)

    def command(self, name, argv, timeout):
        quiescent(self.scope)
        record = {"name": name, "argv": argv, "timeout_seconds": timeout,
                  "return_code": None, "signal": None, "timed_out": False}
        self.receipt["commands"].append(record)
        self.write()
        start = time.monotonic()
        stdout = self.out / f"{name}.stdout.log"
        stderr = self.out / f"{name}.stderr.log"
        with stdout.open("wb") as out, stderr.open("wb") as err:
            child = subprocess.Popen(argv, cwd=ROOT, env=self.env, stdout=out, stderr=err,
                                     start_new_session=True)
            try:
                code = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                record["timed_out"] = True
                os.killpg(child.pid, signal.SIGKILL)
                code = child.wait()
        record.update(return_code=code, signal=-code if code < 0 else None,
                      elapsed_seconds=time.monotonic() - start)
        record["logs"] = {p.name: digest(p) for p in [stdout, stderr]}
        self.write()
        quiescent(self.scope)
        require(code == 0 and not record["timed_out"], f"{name} failed: exit={code}")
        return stdout

    def freeze(self, name, path):
        path = Path(path).resolve()
        require(path.is_relative_to(self.args.work.resolve() / "target") and path.is_file(),
                "Cargo executable outside the owned target directory")
        path.chmod(path.stat().st_mode & ~0o222)
        self.receipt["artifacts"][name] = {"path": str(path), "sha256": digest(path),
                                            "elf": elf_identity(path, self.args.arch)}

    def check_artifacts(self):
        for artifact in self.receipt["artifacts"].values():
            path = Path(artifact["path"])
            require(path.is_file() and not path.stat().st_mode & 0o222
                    and digest(path) == artifact["sha256"], "frozen executable changed")

    def run(self):
        try:
            self.scope, self.receipt["scope"] = scope_info(self.args.unit)
            quiescent(self.scope)
            require(resource.getrlimit(resource.RLIMIT_CORE)[0] == 0, "core dumps must be disabled")
            require(platform.system() == "Linux" and platform.machine() == self.args.arch,
                    "runner is not the requested native Linux architecture")
            manifest = json.loads(MANIFEST.read_text())
            self.receipt["tests"] = classified_tests(ROOT, manifest)
            self.receipt["manifest_sha256"] = digest(MANIFEST)
            self.receipt["source_before"] = source_identity(ROOT)
            require(self.receipt["source_before"]["commit"] == self.args.sha, "checkout SHA mismatch")
            work = self.args.work.resolve()
            work.mkdir(parents=True, exist_ok=True)
            require(not (work / "target").exists(), "target directory must be fresh")
            tmp = work / "tmp"
            tmp.mkdir()
            fstype = output(["findmnt", "--noheadings", "--output", "FSTYPE", "--target", str(tmp)])
            require(fstype not in ["tmpfs", "ramfs"], "TMPDIR must be on disk")
            channel = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
            self.env = {k: os.environ[k] for k in ["PATH", "HOME", "CARGO_HOME", "RUSTUP_HOME"]
                        if k in os.environ}
            self.env.update(TMPDIR=str(tmp), CARGO_TARGET_DIR=str(work / "target"),
                            CARGO_BUILD_JOBS="2", RAYON_NUM_THREADS="2", CARGO_INCREMENTAL="0",
                            RUSTUP_TOOLCHAIN=channel, LANG="C.UTF-8")
            rust = output(["rustc", "-vV"], env=self.env)
            triple = TRIPLES[self.args.arch]
            require(f"host: {triple}" in rust, "Rust host target does not match the native runner")
            z3 = output(["z3", "--version"], env=self.env)
            require(z3.startswith("Z3 version 4.15.4 "), "Z3 reference version mismatch")
            self.receipt["environment"] = {
                "system": platform.system(), "machine": platform.machine(),
                "kernel": platform.release(), "libc": platform.libc_ver(),
                "image_os": os.environ.get("ImageOS"), "image_version": os.environ.get("ImageVersion"),
                "rustc": rust, "cargo": output(["cargo", "--version"], env=self.env),
                "z3": z3, "z3_binary_sha256": digest(shutil.which("z3", path=self.env["PATH"])),
                "channel": channel, "target": triple, "tmp_fstype": fstype,
                "disk_free_bytes": shutil.disk_usage(work).free,
                "features": "default", "rust_min_stack": "unset", "payload_env": self.env}
            self.receipt["memory_events_before"] = (self.scope / "memory.events").read_text()
            self.write()
            self.command("harness-controls", CONTROL_ARGV, 120)
            executables = {}
            for name, argv in build_commands(manifest):
                log = self.command(name, argv, 3600)
                wanted = ["anubis"] if name == "build-cli" else [
                    t["target"] for t in manifest["targets"] if name == f"compile-{t['package']}"]
                executables.update(cargo_executables(log, wanted, name != "build-cli"))
            self.freeze("anubis", executables["anubis"])
            for target in manifest["targets"]:
                self.freeze(target["target"], executables[target["target"]])
            self.write()
            for target in manifest["targets"]:
                executable = self.receipt["artifacts"][target["target"]]["path"]
                for name in target["tests"]:
                    self.check_artifacts()
                    log = self.command(f"test-{target['target']}-{name}",
                                       [executable, name, "--exact", "--test-threads=1",
                                        "-Z", "unstable-options", "--format=json"], 120)
                    test_result(log, name)
                    self.check_artifacts()
            self.receipt["source_after"] = source_identity(ROOT)
            require(self.receipt["source_before"] == self.receipt["source_after"], "source changed during run")
            self.receipt["memory_events_after"] = (self.scope / "memory.events").read_text()
            require(self.receipt["memory_events_before"] == self.receipt["memory_events_after"],
                    "memory control events changed during run")
            quiescent(self.scope)
            self.receipt["artifacts_after"] = {
                name: {"path": item["path"], "sha256": digest(item["path"]),
                       "elf": elf_identity(item["path"], self.args.arch)}
                for name, item in self.receipt["artifacts"].items()}
            self.receipt.update(verdict=PAYLOAD_COMPLETE, error=None, artifacts_unchanged=True)
            self.write()
            validate(self.out, self.args.sha, self.args.arch, require_launcher=False, require_final=False)
            return 0
        except Exception as error:
            self.receipt.update(verdict="FAIL", error=f"{type(error).__name__}: {error}")
            self.write()
            print(self.receipt["error"], flush=True)
            return 1


def validate(out, expected_sha, expected_arch, require_launcher=True, require_final=True):
    receipt = json.loads((out / "receipt.json").read_text())
    manifest = json.loads(MANIFEST.read_text())
    expected = classified_tests(ROOT, manifest)
    expected_verdict = CLAIM if require_final else PAYLOAD_COMPLETE
    require(receipt.get("schema") == SCHEMA and receipt.get("verdict") == expected_verdict,
            "no complete ordinary receipt for this validation stage")
    require(receipt.get("external") == EXTERNAL, "external requirements changed or missing")
    validate_limits(receipt["scope"]["limits"])
    require(receipt.get("manifest_sha256") == digest(MANIFEST) and receipt.get("tests") == expected,
            "test classification does not match receipt")
    arch = receipt["expected_arch"]
    require(arch == expected_arch and receipt["expected_sha"] == expected_sha,
            "receipt differs from trusted SHA/architecture expectations")
    require(arch in ["x86_64", "aarch64"] and receipt["environment"]["machine"] == arch
            and receipt["environment"]["system"] == "Linux", "native architecture mismatch")
    env = receipt["environment"]
    channel = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    require(env.get("channel") == channel and env.get("target") == TRIPLES[arch]
            and f"host: {TRIPLES[arch]}" in env.get("rustc", "")
            and env.get("cargo", "").startswith("cargo ")
            and env.get("z3", "").startswith("Z3 version 4.15.4 ")
            and re.fullmatch(r"[0-9a-f]{64}", env.get("z3_binary_sha256", "")),
            "tool identity missing or mismatched")
    require(env.get("features") == "default" and env.get("rust_min_stack") == "unset"
            and env.get("tmp_fstype") not in [None, "", "tmpfs", "ramfs"]
            and env.get("payload_env", {}).get("RUSTUP_TOOLCHAIN") == channel
            and "RUST_MIN_STACK" not in env["payload_env"], "execution environment mismatch")
    require(receipt.get("source_before") == receipt.get("source_after")
            and receipt["source_before"]["commit"] == receipt["expected_sha"], "source binding mismatch")
    source = receipt["source_before"]
    require(re.fullmatch(r"[0-9a-f]{40}", source.get("tree", ""))
            and source.get("files") and all(name in source["files"] for name in
                ["Cargo.lock", "rust-toolchain.toml", "scripts/ci/linux_native.py"]),
            "tracked source manifest missing")
    require(receipt.get("memory_events_before") is not None
            and receipt["memory_events_before"] == receipt.get("memory_events_after"),
            "memory control events missing or changed")
    require(receipt.get("artifacts_unchanged") is True, "executable mutation or missing closing check")
    require(receipt.get("artifacts_after") == receipt["artifacts"], "executable closing hashes mismatch")
    required_artifacts = {"anubis", *(t["target"] for t in manifest["targets"])}
    require(set(receipt["artifacts"]) == required_artifacts, "executable roster mismatch")
    for item in receipt["artifacts"].values():
        require(re.fullmatch(r"[0-9a-f]{64}", item["sha256"]) and Path(item["path"]).is_absolute(),
                "invalid executable identity")
        require(item.get("elf") == {"class": "ELF64", "byte_order": "little", "machine": arch},
                "native executable identity mismatch")
    command_names = ["harness-controls", "build-cli", *(f"compile-{p}" for p in dict.fromkeys(t["package"] for t in manifest["targets"])),
                     *(f"test-{t['target']}-{n}" for t in manifest["targets"] for n in t["tests"])]
    commands = receipt["commands"]
    require([r.get("name") for r in commands] == command_names, "command roster mismatch")
    for record in commands:
        require(record.get("return_code") == 0 and record.get("signal") is None
                and record.get("timed_out") is False, "failed command in PASS receipt")
        logs = record["logs"]
        require(set(logs) == {record["name"] + ".stdout.log", record["name"] + ".stderr.log"},
                "missing command log")
        for name, sha in logs.items():
            path = out / name
            require(path.is_file() and not path.is_symlink() and digest(path) == sha, "missing or changed log")
    require(commands[0]["argv"] == CONTROL_ARGV, "control command mismatch")
    require(re.search(r"Ran [1-9][0-9]* tests? in [0-9.]+s\s+OK\s*$",
                      (out / "harness-controls.stderr.log").read_text()),
            "control tests did not report a nonempty successful suite")
    for name, argv in build_commands(manifest):
        record = next(r for r in commands if r["name"] == name)
        require(record["argv"] == argv, "build command differs from approved invocation")
        wanted = ["anubis"] if name == "build-cli" else [
            t["target"] for t in manifest["targets"] if name == f"compile-{t['package']}"]
        exes = cargo_executables(out / f"{name}.stdout.log", wanted, name != "build-cli")
        require(all(receipt["artifacts"][n]["path"] == p for n, p in exes.items()),
                "executed binary does not match Cargo artifact")
    for target in manifest["targets"]:
        for name in target["tests"]:
            test_result(out / f"test-{target['target']}-{name}.stdout.log", name)
            record = next(r for r in commands if r["name"] == f"test-{target['target']}-{name}")
            require(record["argv"] == [receipt["artifacts"][target["target"]]["path"], name,
                                      "--exact", "--test-threads=1", "-Z", "unstable-options", "--format=json"],
                    "test command differs from exact approved invocation")
    launcher = out / "launcher.json"
    if require_launcher:
        require(launcher.is_file() and not launcher.is_symlink(), "launcher/teardown receipt missing")
        launch = json.loads(launcher.read_text())
        require(launch.get("exit_code") == 0 and launch.get("unit_removed") is True
                and launch.get("query_exit_code") == 0
                and launch.get("launch_exit_code") == "0" and launch.get("cleanup_required") is False
                and launch.get("load_state_after") == "not-found"
                and launch.get("unit") == receipt["scope"]["unit"],
                "bounded service failed or did not tear down")
        if require_final:
            require(launch.get("validation_exit_code") == 0, "final source/receipt validation missing or failed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run")
    run.add_argument("--arch", choices=["x86_64", "aarch64"], required=True)
    for name in ["sha", "unit"]:
        run.add_argument(f"--{name}", required=True)
    run.add_argument("--work", type=Path, required=True)
    run.add_argument("--out", type=Path, required=True)
    for verb in ["validate", "finalize"]:
        check = sub.add_parser(verb)
        check.add_argument("--out", type=Path, required=True)
        check.add_argument("--sha", required=True)
        check.add_argument("--arch", choices=["x86_64", "aarch64"], required=True)
    args = parser.parse_args()
    if args.command == "run":
        return Driver(args).run()
    if args.command == "finalize":
        launch = json.loads((args.out / "launcher.json").read_text())
        try:
            validate(args.out, args.sha, args.arch, require_final=False)
            require(json.loads((args.out / "receipt.json").read_text())["source_after"] == source_identity(ROOT),
                    "receipt source manifest differs from current checkout")
        except Exception as error:
            launch["validation_exit_code"] = 1
            save(args.out / "launcher.json", launch)
            receipt = json.loads((args.out / "receipt.json").read_text())
            receipt.update(verdict="FAIL", error=f"final validation failed: {error}")
            save(args.out / "receipt.json", receipt)
            raise
        launch["validation_exit_code"] = 0
        save(args.out / "launcher.json", launch)
        receipt = json.loads((args.out / "receipt.json").read_text())
        receipt["verdict"] = CLAIM
        save(args.out / "receipt.json", receipt)
    else:
        validate(args.out, args.sha, args.arch)
        require(json.loads((args.out / "receipt.json").read_text())["source_after"] == source_identity(ROOT),
                "receipt source manifest differs from current checkout")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
