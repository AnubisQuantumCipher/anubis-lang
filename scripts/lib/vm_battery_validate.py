#!/usr/bin/env python3
"""Strict ordered state-machine validator for a VZ guest battery log."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
from pathlib import Path
from typing import NoReturn

EXPECTED_GATES = (
    "pin-smoke", "cargo-test", "tool-test", "clippy", "build-rel", "language", "turing",
    "security", "stdlib", "shadow", "seal", "dogfood", "effect-sh", "capset-sh", "type-sh",
    "taint-sh", "stdlib-fc", "native-auth", "docs-drift", "walker", "formal", "formal-kernel",
    "correspondence",
)
EXPECTED_GATE_COUNT = 23
EXPECTED_GATE_ROSTER_SHA256 = "c2149a53575e39d2651a79f7240e4a459ed672769eaae252e4c4d1a81961bc25"
HEADER = re.compile(r"^ANUBIS_VM_GATE_BEGIN ([a-z0-9-]+)$")
RESULT = re.compile(r"^ANUBIS_VM_GATE_RESULT ([0-9]+) ([a-z0-9-]+)$")
PROTOCOL_MAGIC = "ANUBIS_VM_PROTOCOL_V1"
SEAL_FIXPOINT = re.compile(r"^ANUBIS_VM_SEAL_FIXPOINT ([0-9a-f]{64})$")
LOG_BINDING = re.compile(r"^ANUBIS_VM_LOG_SHA256 ([0-9a-f]{64}) ([0-9]+)$")
JOBS = re.compile(r"^ANUBIS_VM_BUILD_JOBS=([0-9]+)$")
PIN_IDENTITY = re.compile(
    r"^ANUBIS_VM_SELECTED_PIN "
    r"(vm/pins/anubis-[0-9a-f]{12}(?:-src-[0-9a-f]{12}(?:-release)?)?) "
    r"([0-9a-f]{64}) ([0-9a-f]{64})$"
)
PIN_PATH = re.compile(
    r"^vm/pins/anubis-[0-9a-f]{12}(?:-src-[0-9a-f]{12}(?:-release)?)?$"
)


def gate_roster_sha256(roster: tuple[str, ...]) -> str:
    return hashlib.sha256(b"\0".join(name.encode("ascii") for name in roster)).hexdigest()


def expected_gate_roster_errors(roster: tuple[str, ...]) -> list[str]:
    errors: list[str] = []
    if not isinstance(roster, tuple):
        return ["EXPECTED_GATES must be a tuple"]
    if len(roster) != EXPECTED_GATE_COUNT:
        errors.append(f"EXPECTED_GATES count is {len(roster)}, expected {EXPECTED_GATE_COUNT}")
    if len(set(roster)) != len(roster):
        errors.append("EXPECTED_GATES contains duplicate names")
    invalid = [
        name
        for name in roster
        if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9-]+", name)
    ]
    if invalid:
        errors.append(f"EXPECTED_GATES contains invalid names: {invalid!r}")
    try:
        digest = gate_roster_sha256(roster)
    except (AttributeError, UnicodeEncodeError):
        digest = "<unavailable>"
    if digest != EXPECTED_GATE_ROSTER_SHA256:
        errors.append(
            f"EXPECTED_GATES digest is {digest}, expected {EXPECTED_GATE_ROSTER_SHA256}"
        )
    return errors


def fail(message: str) -> NoReturn:
    print(f"VM_BATTERY_VALIDATOR_ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


def regular_nonsymlink(path: Path, label: str) -> None:
    try:
        mode = path.lstat().st_mode
    except OSError as exc:
        fail(f"cannot stat {label} {path}: {exc}")
    if stat.S_ISLNK(mode) or not stat.S_ISREG(mode):
        fail(f"{label} must be a regular non-symlink file: {path}")


def atomic_json(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        fail(f"verdict output must not be a symlink: {path}")
    temp = path.with_name(f".{path.name}.tmp.{os.getpid()}")
    if temp.exists() or temp.is_symlink():
        fail(f"temporary verdict already exists: {temp}")
    data = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    try:
        with temp.open("x", encoding="utf-8") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp, path)
    except OSError as exc:
        fail(f"cannot publish verdict: {exc}")
    finally:
        try:
            temp.unlink()
        except FileNotFoundError:
            pass


def invalidate_previous_verdict(path: Path) -> None:
    """Remove a prior verdict before this invocation can fail and leave stale PASS."""
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        return
    except OSError as exc:
        fail(f"cannot stat prior verdict output {path}: {exc}")
    if stat.S_ISDIR(mode):
        fail(f"verdict output must not be a directory: {path}")
    try:
        path.unlink()
    except OSError as exc:
        fail(f"cannot invalidate prior verdict output {path}: {exc}")


def main() -> int:
    # Discover the verdict target before the authoritative parse. argparse can reject an unrelated
    # value (for example, a non-integer --expected-jobs) before returning `args`; if invalidation
    # waits until afterward, that early exit leaves a prior PASS at the advertised output path.
    # This parser deliberately knows only --out and ignores the rest. With no output path there is
    # no identified verdict to invalidate, and the full parser below retains normal --help/errors.
    output_parser = argparse.ArgumentParser(add_help=False)
    output_parser.add_argument("--out", type=Path)
    output_args, _ = output_parser.parse_known_args()
    if output_args.out is not None:
        invalidate_previous_verdict(output_args.out)

    roster_errors = expected_gate_roster_errors(EXPECTED_GATES)
    if roster_errors:
        fail("; ".join(roster_errors))
    parser = argparse.ArgumentParser()
    parser.add_argument("--log", required=True, type=Path)
    parser.add_argument("--protocol", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--expected-fixpoint", required=True)
    parser.add_argument("--expected-jobs", required=True, type=int)
    parser.add_argument("--expected-pin", required=True)
    parser.add_argument("--expected-pin-sha256", required=True)
    parser.add_argument("--expected-pin-meta-sha256", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{64}", args.expected_fixpoint):
        fail("--expected-fixpoint must be 64 lowercase hex characters")
    if not 1 <= args.expected_jobs <= 6:
        fail("--expected-jobs must be in 1..6")
    if not PIN_PATH.fullmatch(args.expected_pin):
        fail("--expected-pin is not a canonical repository-relative immutable pin path")
    for label, value in (
        ("--expected-pin-sha256", args.expected_pin_sha256),
        ("--expected-pin-meta-sha256", args.expected_pin_meta_sha256),
    ):
        if not re.fullmatch(r"[0-9a-f]{64}", value):
            fail(f"{label} must be 64 lowercase hex characters")
    regular_nonsymlink(args.log, "battery log")
    regular_nonsymlink(args.protocol, "battery protocol")
    try:
        raw = args.log.read_bytes()
        raw.decode("utf-8")
        protocol_raw = args.protocol.read_bytes()
        protocol_text = protocol_raw.decode("utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        fail(f"cannot read battery evidence exactly: {exc}")
    lines = protocol_text.splitlines()
    errors: list[str] = []
    observed: list[str] = []
    exits: dict[str, int] = {}
    active: str | None = None
    expected_index = 0
    done_count = 0
    magic_count = 0
    jobs_values: list[int] = []
    pin_identities: list[tuple[str, str, str]] = []
    fixpoints: list[str] = []
    log_bindings: list[tuple[str, int]] = []

    for line_number, line in enumerate(lines, start=1):
        if line == PROTOCOL_MAGIC:
            magic_count += 1
            if line_number != 1 or magic_count > 1:
                errors.append(f"line {line_number}: misplaced or duplicate protocol magic")
            continue
        if line.startswith("ANUBIS_VM_PROTOCOL"):
            errors.append(f"line {line_number}: malformed protocol magic: {line!r}")
            continue
        jobs_match = JOBS.fullmatch(line)
        if jobs_match:
            jobs_values.append(int(jobs_match.group(1)))
            if line_number != 2 or expected_index or active is not None:
                errors.append(f"line {line_number}: VM_BUILD_JOBS marker is not the launcher preamble")
            continue
        if line.startswith("ANUBIS_VM_BUILD_JOBS"):
            errors.append(f"line {line_number}: malformed VM_BUILD_JOBS marker: {line!r}")
            continue
        pin_match = PIN_IDENTITY.fullmatch(line)
        if pin_match:
            pin_identities.append(
                (pin_match.group(1), pin_match.group(2), pin_match.group(3))
            )
            if line_number != 3 or expected_index or active is not None:
                errors.append(
                    f"line {line_number}: selected-pin marker is not the launcher preamble"
                )
            continue
        if line.startswith("ANUBIS_VM_SELECTED_PIN"):
            errors.append(f"line {line_number}: malformed selected-pin marker: {line!r}")
            continue
        if line.startswith("ANUBIS_VM_SEAL_FIXPOINT"):
            fixpoint = SEAL_FIXPOINT.fullmatch(line)
            if not fixpoint:
                errors.append(f"line {line_number}: malformed seal fixpoint: {line!r}")
            else:
                fixpoints.append(fixpoint.group(1))
                if active != "seal":
                    errors.append(f"line {line_number}: seal fixpoint emitted while active gate is {active!r}")
            continue
        if line.startswith("ANUBIS_VM_LOG_SHA256"):
            binding = LOG_BINDING.fullmatch(line)
            if not binding:
                errors.append(f"line {line_number}: malformed log binding: {line!r}")
            else:
                log_bindings.append((binding.group(1), int(binding.group(2))))
                if active is not None or expected_index != len(EXPECTED_GATES) or done_count:
                    errors.append(f"line {line_number}: log binding is not after the exact gate roster")
            continue
        if line.startswith("ANUBIS_VM_GATE_BEGIN"):
            header = HEADER.fullmatch(line)
            if not header:
                errors.append(f"line {line_number}: malformed gate header: {line!r}")
                continue
            name = header.group(1)
            if done_count:
                errors.append(f"line {line_number}: gate header after BATTERY_DONE: {name}")
            if active is not None:
                errors.append(f"line {line_number}: header {name} before result for {active}")
            if name not in EXPECTED_GATES:
                errors.append(f"line {line_number}: unknown gate header: {name}")
            if expected_index >= len(EXPECTED_GATES) or name != EXPECTED_GATES[expected_index]:
                wanted = EXPECTED_GATES[expected_index] if expected_index < len(EXPECTED_GATES) else "<none>"
                errors.append(f"line {line_number}: gate order mismatch: observed {name}, expected {wanted}")
            active = name
            observed.append(name)
            continue
        if line.startswith("ANUBIS_VM_GATE_RESULT"):
            result = RESULT.fullmatch(line)
            if not result:
                errors.append(f"line {line_number}: malformed EXIT marker: {line!r}")
                continue
            rc = int(result.group(1))
            name = result.group(2)
            if done_count:
                errors.append(f"line {line_number}: result after BATTERY_DONE: {name}")
            if name in exits:
                errors.append(f"line {line_number}: duplicate result for {name}")
            if active is None:
                label = "duplicate" if name in exits else "before header"
                errors.append(f"line {line_number}: result for {name} {label}")
            elif name != active:
                errors.append(f"line {line_number}: result for {name} while active gate is {active}")
            else:
                active = None
                if expected_index < len(EXPECTED_GATES) and name == EXPECTED_GATES[expected_index]:
                    expected_index += 1
            exits.setdefault(name, rc)
            if rc != 0:
                errors.append(f"line {line_number}: nonzero exit {rc} for {name}")
            continue
        if line == "ANUBIS_VM_BATTERY_DONE":
            done_count += 1
            if done_count > 1:
                errors.append(f"line {line_number}: duplicate BATTERY_DONE marker")
            if active is not None:
                errors.append(f"line {line_number}: BATTERY_DONE before result for {active}")
            if expected_index != len(EXPECTED_GATES):
                errors.append(
                    f"line {line_number}: BATTERY_DONE after {expected_index}/{len(EXPECTED_GATES)} ordered gates"
                )
            if len(log_bindings) != 1:
                errors.append(
                    f"line {line_number}: BATTERY_DONE with {len(log_bindings)} log binding(s), expected 1"
                )
            continue
        if line.startswith("ANUBIS_VM_BATTERY_DONE"):
            errors.append(f"line {line_number}: malformed BATTERY_DONE marker: {line!r}")
            continue
        errors.append(f"line {line_number}: unknown protocol record: {line!r}")

    if magic_count != 1:
        errors.append(f"protocol magic count is {magic_count}, expected 1")
    if active is not None:
        errors.append(f"end of protocol before result for {active}")
    if done_count != 1:
        errors.append(f"BATTERY_DONE count is {done_count}, expected 1")
    missing = [name for name in EXPECTED_GATES if name not in exits]
    extra = sorted(set(exits) - set(EXPECTED_GATES))
    if missing:
        errors.append(f"missing gate result(s): {', '.join(missing)}")
    if extra:
        errors.append(f"unknown gate result(s): {', '.join(extra)}")
    if observed != list(EXPECTED_GATES):
        errors.append("observed gate header sequence does not equal the exact expected roster")
    if len(jobs_values) != 1 or jobs_values[0] != args.expected_jobs:
        errors.append(f"VM_BUILD_JOBS markers={jobs_values!r}, expected exactly [{args.expected_jobs}]")
    expected_pin_identity = (
        args.expected_pin,
        args.expected_pin_sha256,
        args.expected_pin_meta_sha256,
    )
    if len(pin_identities) != 1 or pin_identities[0] != expected_pin_identity:
        errors.append(
            "selected-pin markers="
            f"{pin_identities!r}, expected exactly [{expected_pin_identity!r}]"
        )
    if len(fixpoints) != 1:
        errors.append(f"seal fixpoint count is {len(fixpoints)}, expected exactly 1")
    elif fixpoints[0] != args.expected_fixpoint:
        errors.append(f"fixpoint mismatch: observed {fixpoints[0]}, expected {args.expected_fixpoint}")
    actual_log_sha256 = hashlib.sha256(raw).hexdigest()
    actual_log_bytes = len(raw)
    if len(log_bindings) != 1:
        errors.append(f"log binding count is {len(log_bindings)}, expected exactly 1")
    elif log_bindings[0] != (actual_log_sha256, actual_log_bytes):
        errors.append(
            "child log binding mismatch: "
            f"protocol={log_bindings[0]!r} actual={(actual_log_sha256, actual_log_bytes)!r}"
        )

    payload: dict[str, object] = {
        "schema": "anubis.vm-battery-verdict.v2",
        "verdict": "PASS" if not errors else "FAIL",
        "expected_gates": list(EXPECTED_GATES),
        "expected_gate_roster_sha256": gate_roster_sha256(EXPECTED_GATES),
        "observed_gates": observed,
        "exit_codes": exits,
        "missing_gates": missing,
        "unknown_gates": extra,
        "battery_done_count": done_count,
        "vm_build_jobs": jobs_values,
        "expected_jobs": args.expected_jobs,
        "selected_pin_identities": [list(identity) for identity in pin_identities],
        "expected_pin_identity": list(expected_pin_identity),
        "fixpoints": fixpoints,
        "expected_fixpoint": args.expected_fixpoint,
        "log_binding": [list(binding) for binding in log_bindings],
        "log_sha256": actual_log_sha256,
        "log_bytes": actual_log_bytes,
        "protocol_log_sha256": hashlib.sha256(protocol_raw).hexdigest(),
        "protocol_log_bytes": len(protocol_raw),
        "errors": errors,
    }
    atomic_json(args.out, payload)
    if errors:
        print(f"VM_BATTERY_VALIDATE_FAIL errors={len(errors)} verdict={args.out}")
        return 1
    print(f"VM_BATTERY_VALIDATE_PASS gates={len(EXPECTED_GATES)} verdict={args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
