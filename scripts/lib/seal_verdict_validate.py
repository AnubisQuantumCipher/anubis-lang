#!/usr/bin/env python3
"""Validate final seal_verdict.json against an exact gate roster."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any, NoReturn

CORE_GATES = (
    "capset_registry_parity",
    "capset_selfhost",
    "check_run_parity",
    "corpus_inventory_binding",
    "docs_drift",
    "formal",
    "formal_kernel",
    "gate_common_adoption",
    "gate_run_freshness",
    "host_resource_contract",
    "instrument_hygiene",
    "language",
    "native_authoritative",
    "proof_correspondence",
    "run_failclosed",
    "runtime",
    "security",
    "selfhost",
    "stdlib_failclosed",
    "taint_selfhost",
)
FULL_EXTRA = (
    "effect_selfhost",
    "selfhost_dogfood",
    "selfhost_fulllang",
    "type_selfhost",
)
LEDGER_BINDING_SCHEMA = "anubis.gate-run-ledger-binding.v1"
SYSTEM_COMMAND_PATH = "/usr/bin:/bin:/usr/sbin:/sbin"


def fail(message: str) -> NoReturn:
    print(f"SEAL_VERDICT_VALIDATE_ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


def expected_for(profile: str) -> tuple[str, ...]:
    if profile == "core":
        return tuple(sorted(CORE_GATES))
    if profile == "full":
        return tuple(sorted(CORE_GATES + FULL_EXTRA))
    fail(f"unknown profile: {profile}")


def load_verdict(path: Path) -> dict[str, Any]:
    try:
        mode = path.lstat().st_mode
    except OSError as exc:
        fail(f"cannot stat verdict: {exc}")
    if stat.S_ISLNK(mode) or not stat.S_ISREG(mode):
        fail(f"verdict must be a regular non-symlink file: {path}")
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        fail(f"cannot parse verdict JSON: {exc}")
    if not isinstance(data, dict):
        fail("verdict JSON root is not an object")
    return data


def read_regular_bytes(path: Path, label: str) -> bytes:
    if not hasattr(os, "O_NOFOLLOW"):
        fail(f"platform lacks O_NOFOLLOW for {label} validation")
    try:
        before = path.lstat()
    except OSError as exc:
        fail(f"cannot stat {label}: {exc}")
    if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
        fail(f"{label} must be a regular non-symlink file: {path}")
    if before.st_mode & 0o222:
        fail(f"{label} must be non-writable during validation: {path}")
    flags = os.O_RDONLY | os.O_NOFOLLOW
    chunks: list[bytes] = []
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb") as handle:
            opened = os.fstat(handle.fileno())
            if not stat.S_ISREG(opened.st_mode):
                fail(f"{label} changed type while opening: {path}")
            while chunk := handle.read(1024 * 1024):
                chunks.append(chunk)
            after = os.fstat(handle.fileno())
    except OSError as exc:
        fail(f"cannot read {label}: {exc}")
    identity_before = (
        before.st_dev,
        before.st_ino,
        before.st_size,
        before.st_mtime_ns,
        before.st_mode,
    )
    identity_opened = (
        opened.st_dev,
        opened.st_ino,
        opened.st_size,
        opened.st_mtime_ns,
        opened.st_mode,
    )
    identity_after = (
        after.st_dev,
        after.st_ino,
        after.st_size,
        after.st_mtime_ns,
        after.st_mode,
    )
    if identity_before != identity_opened or identity_opened != identity_after:
        fail(f"{label} changed while reading: {path}")
    return b"".join(chunks)


def trusted_git() -> Path:
    candidate = shutil.which("git", path=SYSTEM_COMMAND_PATH)
    if candidate is None:
        fail("trusted system Git executable is unavailable")
    path = Path(candidate)
    try:
        metadata = path.lstat()
    except OSError as exc:
        fail(f"cannot stat trusted system Git executable: {exc}")
    if (
        not path.is_absolute()
        or stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISREG(metadata.st_mode)
        or not os.access(path, os.X_OK)
        or metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH)
    ):
        fail(f"trusted system Git path is unsafe: {path}")
    return path


def read_head(repo_root: Path) -> str:
    if not repo_root.is_absolute():
        fail("--repo-root must be absolute")
    try:
        metadata = repo_root.lstat()
    except OSError as exc:
        fail(f"cannot stat repository root: {exc}")
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
        fail(f"repository root must be a real directory: {repo_root}")
    environment = {
        name: value for name, value in os.environ.items() if not name.startswith("GIT_")
    }
    environment["PATH"] = SYSTEM_COMMAND_PATH
    environment["LC_ALL"] = "C"
    environment["GIT_OPTIONAL_LOCKS"] = "0"
    result = subprocess.run(
        [str(trusted_git()), "-C", str(repo_root), "rev-parse", "--verify", "HEAD^{commit}"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
        env=environment,
    )
    head = result.stdout.strip()
    if result.returncode != 0 or re.fullmatch(r"[0-9a-f]{40}", head) is None:
        fail(
            "cannot read an exact repository HEAD with trusted system Git "
            f"(rc={result.returncode})"
        )
    return head


def validate_ledger_binding(
    data: dict[str, Any], ledger_path: Path, profile: str, errors: list[str]
) -> str:
    raw = read_regular_bytes(ledger_path, "gate-run ledger")
    digest = hashlib.sha256(raw).hexdigest()
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        errors.append("gate-run ledger is not ASCII")
        return ""
    if not text.endswith("\n") or "\r" in text:
        errors.append("gate-run ledger must end in one LF-delimited row format")
    rows = text.splitlines()
    expected_names = [name for name in expected_for(profile) if name != "gate_run_freshness"]
    names: list[str] = []
    commits: list[str] = []
    for index, row in enumerate(rows, start=1):
        fields = row.split(" ")
        if (
            len(fields) != 3
            or not fields[0]
            or re.fullmatch(r"[0-9a-f]{40}", fields[1]) is None
            or re.fullmatch(r"[0-9]+", fields[2]) is None
        ):
            errors.append(f"malformed gate-run ledger row {index}")
            continue
        names.append(fields[0])
        commits.append(fields[1])
    if names != expected_names:
        errors.append(
            f"gate-run ledger roster mismatch: actual={names!r} expected={expected_names!r}"
        )
    unique_commits = sorted(set(commits))
    if len(unique_commits) != 1:
        errors.append(f"gate-run ledger must contain one commit epoch: {unique_commits!r}")
    commit = unique_commits[0] if len(unique_commits) == 1 else ""

    binding = data.get("gate_run_ledger")
    if not isinstance(binding, dict):
        errors.append("gate_run_ledger binding is missing or not an object")
        return commit
    if binding.get("schema") != LEDGER_BINDING_SCHEMA:
        errors.append(f"unexpected gate-run ledger binding schema: {binding.get('schema')!r}")
    if binding.get("sha256") != digest:
        errors.append(
            f"gate-run ledger sha256 mismatch: {binding.get('sha256')!r} != {digest!r}"
        )
    if binding.get("rows") != len(expected_names):
        errors.append(
            f"gate-run ledger bound row count {binding.get('rows')!r} does not match {len(expected_names)}"
        )
    if binding.get("commit") != commit:
        errors.append(
            f"gate-run ledger bound commit {binding.get('commit')!r} does not match {commit!r}"
        )
    if binding.get("promoted_name") != "gate_run_ledger.validated":
        errors.append(
            f"unexpected promoted ledger name: {binding.get('promoted_name')!r}"
        )
    return commit


def detail_int(detail: str, key: str) -> int | None:
    match = re.search(rf"(?:^| ){re.escape(key)}=([0-9]+)(?: |$)", detail)
    if not match:
        return None
    return int(match.group(1))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verdict", type=Path)
    parser.add_argument("--ledger", type=Path)
    parser.add_argument("--repo-root", type=Path)
    parser.add_argument("--profile", required=True, choices=("core", "full"))
    parser.add_argument("--print-roster", action="store_true")
    args = parser.parse_args()
    if args.print_roster:
        if args.verdict is not None or args.ledger is not None or args.repo_root is not None:
            fail("--print-roster cannot be combined with --verdict, --ledger, or --repo-root")
        for gate in expected_for(args.profile):
            print(gate)
        return 0
    if args.verdict is None:
        fail("--verdict is required unless --print-roster is used")
    if args.ledger is None:
        fail("--ledger is required when validating a verdict")
    data = load_verdict(args.verdict)
    errors: list[str] = []
    expected = expected_for(args.profile)
    head_before = read_head(args.repo_root) if args.repo_root is not None else ""
    ledger_commit = validate_ledger_binding(data, args.ledger, args.profile, errors)
    if args.repo_root is not None:
        head_after = read_head(args.repo_root)
        if head_before != head_after:
            errors.append(
                f"repository HEAD changed during ledger validation: {head_before!r} != {head_after!r}"
            )
        if ledger_commit and ledger_commit != head_before:
            errors.append(
                f"gate-run ledger commit {ledger_commit!r} does not equal repository HEAD {head_before!r}"
            )
    if data.get("gate") != "seal_checklist":
        errors.append(f"gate field is not seal_checklist: {data.get('gate')!r}")
    if data.get("status") != "SEAL_PASS":
        errors.append(f"status is not SEAL_PASS: {data.get('status')!r}")
    if data.get("profile") != args.profile:
        errors.append(f"profile mismatch: {data.get('profile')!r} != {args.profile!r}")
    scoring = data.get("scoring_rule")
    if scoring != "declared_verdict_line_only_never_body_grep_FAIL":
        errors.append(f"unexpected scoring_rule: {scoring!r}")
    instrument = data.get("instrument")
    if not isinstance(instrument, dict) or not isinstance(instrument.get("raw"), str) or "seal_instrument_v1" not in instrument["raw"]:
        errors.append("instrument raw payload missing seal_instrument_v1")
    detail = data.get("detail")
    if not isinstance(detail, str):
        errors.append("detail field is not a string")
        detail = ""
    if "skip=0" not in detail:
        errors.append("detail does not record skip=0")
    if "known_fail=0" not in detail:
        errors.append("detail does not record known_fail=0")
    if not re.search(r"sha256=[0-9a-f]{64}(?: |$)", detail):
        errors.append("detail missing final 64-hex sha256")
    detail_pass = detail_int(detail, "pass")
    if detail_pass != len(expected):
        errors.append(f"detail pass count {detail_pass!r} does not match expected roster size {len(expected)}")
    gates = data.get("gates")
    if not isinstance(gates, list):
        errors.append("gates is not a list")
        gates = []
    names: list[str] = []
    for index, row in enumerate(gates):
        if not isinstance(row, dict):
            errors.append(f"gate row {index} is not an object")
            continue
        name = row.get("name")
        if not isinstance(name, str) or not name:
            errors.append(f"gate row {index} has invalid name {name!r}")
            continue
        names.append(name)
        if row.get("status") != "PASS":
            errors.append(f"gate {name} has non-PASS status {row.get('status')!r}")
        if not isinstance(row.get("declared_verdict_line"), str) or not row.get("declared_verdict_line"):
            errors.append(f"gate {name} missing declared verdict line")
        if not isinstance(row.get("score_reason"), str) or not row.get("score_reason"):
            errors.append(f"gate {name} missing score reason")
    seen: set[str] = set()
    duplicates: list[str] = []
    for name in names:
        if name in seen:
            duplicates.append(name)
        seen.add(name)
    if duplicates:
        errors.append(f"duplicate gate row(s): {sorted(set(duplicates))}")
    actual = set(names)
    expected_set = set(expected)
    missing = sorted(expected_set - actual)
    extra = sorted(actual - expected_set)
    if missing:
        errors.append(f"missing expected gate row(s): {missing}")
    if extra:
        errors.append(f"extra unexpected gate row(s): {extra}")
    if len(names) != len(expected):
        errors.append(f"gate row count {len(names)} does not match expected {len(expected)}")
    if errors:
        print(f"SEAL_VERDICT_VALIDATE_FAIL errors={len(errors)}")
        for error in errors:
            print(f"  {error}")
        return 1
    print(f"SEAL_VERDICT_VALIDATE_PASS profile={args.profile} gates={len(expected)} verdict={args.verdict}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
