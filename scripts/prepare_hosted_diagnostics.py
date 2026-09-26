#!/usr/bin/env python3
"""Retain only approved minimized CI surfaces as diagnostic, never attestation.

This command's success means packaging succeeded. It does not validate gate
results and must not replace the hosted success-attestation validator.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tempfile


APPROVED_FILES = (
    "gate_report.json",
    "gate_log.txt",
    "profile_environment.txt",
    "attestation_identity.txt",
)
IDENTITY_ENV = (
    "GITHUB_REPOSITORY",
    "GITHUB_WORKFLOW",
    "GITHUB_RUN_ID",
    "GITHUB_RUN_ATTEMPT",
    "GITHUB_EVENT_NAME",
    "GITHUB_REF",
    "GITHUB_SHA",
)

MAX_LINE_BYTES = 4096
MAX_ENTRIES = 256
MAX_NAME_BYTES = 512
TEST_ROW = re.compile(rb"test ([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*) \.\.\. FAILED")
SUITE_START = re.compile(rb"running [0-9]+ tests?")
SUITE_END = re.compile(rb"test result: (?:ok|FAILED)\. [0-9]+ passed; [0-9]+ failed; .*")
RUST_ERROR = re.compile(rb"error\[(E[0-9]{4})\]:")
SIGNAL_END = re.compile(rb"  process didn't exit successfully: .+ \(signal: [0-9]+, (SIGABRT|SIGSEGV|SIGBUS|SIGKILL|SIGTERM|SIGILL|SIGFPE|SIGTRAP|SIGPIPE): [^\r\n]*\)")


def open_regular(source):
    """Open an observed regular file without following a replaced leaf link."""
    before = source.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError("nonregular diagnostic input")
    descriptor = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    reader = os.fdopen(descriptor, "rb")
    opened = os.fstat(reader.fileno())
    if not stat.S_ISREG(opened.st_mode) or (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
        reader.close()
        raise ValueError("diagnostic input changed before opening")
    return reader, opened


def g3_diagnostics(source):
    result = {
        "command": ["cargo", "test", "--all"],
        "authority": "unvalidated_log_observations",
        "exit_status": None,
        "exit_status_state": "missing",
        "raw_log": {"state": "missing"},
        "failed_tests": [],
        "rust_error_codes": [],
        "termination_categories": [],
        "extraction_truncated": False,
        "extraction_stopped_at_captured_output": False,
    }
    try:
        reader, _ = open_regular(source / "g3_test.exit")
        with reader:
            raw_exit = reader.read(32)
        if re.fullmatch(rb"(?:0|[1-9][0-9]{0,2})\n", raw_exit) and int(raw_exit) <= 255:
            result["exit_status"] = int(raw_exit)
            result["exit_status_state"] = "observed"
        else:
            result["exit_status_state"] = "malformed"
    except FileNotFoundError:
        pass
    except (OSError, ValueError):
        result["exit_status_state"] = "unreadable_or_nonregular"

    try:
        reader, opened = open_regular(source / "g3_test.log")
    except FileNotFoundError:
        return result
    except (OSError, ValueError):
        result["raw_log"] = {"state": "omitted", "reason": "unreadable_or_nonregular"}
        return result

    def add(key, value):
        if value in result[key]:
            return
        if len(result[key]) >= MAX_ENTRIES:
            result["extraction_truncated"] = True
        else:
            result[key].append(value)

    digest = hashlib.sha256()
    phase = "outside"
    dropping_line = False
    with reader:
        while chunk := reader.readline(MAX_LINE_BYTES + 1):
            digest.update(chunk)
            if dropping_line or len(chunk) > MAX_LINE_BYTES:
                result["extraction_truncated"] = True
                dropping_line = not chunk.endswith(b"\n")
                continue
            line = chunk.rstrip(b"\r\n")
            # After captured output begins, even a plausible summary/header can
            # be arbitrary test text. Keep hashing, but never resume extraction.
            if phase == "captured":
                continue
            if SUITE_END.fullmatch(line):
                phase = "outside"
                continue
            # Rust's captured assertion/stdout text is arbitrary. Do not mine
            # it for test IDs, compiler codes or process termination claims.
            if line == b"failures:" or line.startswith(b"---- "):
                phase = "captured"
                result["extraction_stopped_at_captured_output"] = True
                continue
            if SUITE_START.fullmatch(line) and phase == "outside":
                phase = "listing"
                continue
            if phase == "listing":
                match = TEST_ROW.fullmatch(line)
                if match:
                    if len(match[1]) <= MAX_NAME_BYTES:
                        add("failed_tests", match[1].decode("ascii"))
                        add("termination_categories", "libtest_failure_reported")
                    else:
                        result["extraction_truncated"] = True
            if phase == "outside":
                match = RUST_ERROR.match(line)
                if match:
                    add("rust_error_codes", match[1].decode("ascii"))
                    add("termination_categories", "rustc_error_reported")
            match = SIGNAL_END.fullmatch(line)
            if match:
                add("termination_categories", "cargo_reported_" + match[1].decode("ascii"))
        after = os.fstat(reader.fileno())
        if (opened.st_size, opened.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError("G3 diagnostic input changed while reading")
    result["raw_log"] = {"state": "hashed_not_published", "sha256": digest.hexdigest()}
    if result["exit_status"] not in (None, 0) and not result["termination_categories"]:
        add("termination_categories", "unclassified_nonzero_exit")
    return result


def reject_symlink_components(path):
    for component in (path, *path.parents):
        if component.is_symlink():
            raise ValueError("diagnostic path has a symlink component")


def retain_file(source, destination):
    try:
        before = source.lstat()
    except FileNotFoundError:
        return {"state": "missing"}
    if not stat.S_ISREG(before.st_mode):
        return {"state": "omitted", "reason": "nonregular_or_symlink"}
    if before.st_size == 0:
        return {"state": "omitted", "reason": "empty"}

    # NOFOLLOW prevents the last component becoming a symlink after lstat.
    descriptor = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as reader:
        opened = os.fstat(reader.fileno())
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise ValueError("diagnostic input changed before opening")
        if not stat.S_ISREG(opened.st_mode):
            raise ValueError("diagnostic input is not a regular file")
        digest = hashlib.sha256()
        with destination.open("xb") as writer:
            while chunk := reader.read(65536):
                writer.write(chunk)
                digest.update(chunk)
        after = os.fstat(reader.fileno())
        if (opened.st_size, opened.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise ValueError("diagnostic input changed while copying")
    return {"state": "retained", "sha256": digest.hexdigest()}


def prepare(source, output, identity):
    source = Path(source).absolute()
    output = Path(output).absolute()
    reject_symlink_components(source)
    reject_symlink_components(output)
    if source.exists() and not source.is_dir():
        raise ValueError("diagnostic source is not a directory")
    if output.exists():
        raise ValueError("diagnostic destination already exists")
    output.parent.mkdir(parents=True, exist_ok=True)
    reject_symlink_components(output.parent)

    # No partial directory is published if a copy or validation fails.
    with tempfile.TemporaryDirectory(prefix=".hosted-diagnostics-", dir=output.parent) as name:
        staging = Path(name)
        records = {}
        for filename in APPROVED_FILES:
            # Existing release consumers recognize root success-artifact names.
            # Diagnostic bytes must not satisfy that layout, even if a retained
            # unvalidated report falsely says HOSTED_PASS.
            records[filename] = retain_file(source / filename, staging / ("unvalidated-" + filename))
        marker = {
            "schema": "anubis.hosted-ci-diagnostics.v1",
            "purpose": "failure_diagnosis_only",
            "is_release_attestation": False,
            "gate_validation": "not_performed",
            "identity": {key: identity.get(key, "") for key in IDENTITY_ENV},
            "files": records,
            "raw_gate_logs": "excluded",
            "g3": g3_diagnostics(source),
            "non_claims": [
                "No gate or release approval follows from this artifact.",
                "Retained report fields are unvalidated diagnostic input.",
                "This is not complete Cargo or gate-log retention.",
                "G3 fields recognize bounded syntactic log fragments, not authenticated failures.",
                "Missing extracted details do not establish absence of a failure.",
            ],
        }
        (staging / "DIAGNOSTIC_ONLY.json").write_text(
            json.dumps(marker, sort_keys=True, indent=2) + "\n", encoding="utf-8"
        )
        lines = []
        for path in sorted(staging.iterdir()):
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            lines.append(f"{digest}  {path.name}\n")
        (staging / "MANIFEST.sha256").write_text("".join(lines), encoding="utf-8")
        staging.rename(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    try:
        prepare(args.source, args.out, os.environ)
    except (OSError, ValueError) as error:
        parser.exit(1, f"hosted diagnostic packaging failed: {type(error).__name__}\n")


if __name__ == "__main__":
    main()
