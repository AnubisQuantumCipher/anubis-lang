#!/usr/bin/env python3
"""Fail-closed final validator for offensive disposable-guest evidence."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import sys
from pathlib import Path
from typing import Any, NoReturn

REQUIRED = ("report.json", "isolation.json", "guest_stdout.log", "export_manifest.json", "teardown_status.txt")
EXPECTED_CASES = (
    "t1_engage_certs", "t1_encrypt_default", "t1_agent_encrypt", "t1_encrypted_c2",
    "t7_console", "t2_launchagent", "t2_inject_plan", "t2_inject_live_double_auth",
    "t3_uds", "t3_dns", "t3_dns_doh_codec", "t7_operator_token_auth",
    "t1_mtls_rustls", "t4_lateral_deny", "t4_lateral_smb_plan", "t7_rbac_queue",
    "t5_pattern", "t5_offset", "t5_browser", "t6_packer", "t6_string_scramble",
    "exploit_run", "doctor_t17", "scope_targets", "t9_attck_catalog", "t9_opsec_score",
    "t9_malleable", "t9_campaign", "t9_phish_plan", "t9_lolbas", "t9_purple_report",
    "t9_recon_hostinfo", "t9_recon_scan", "t9_doctor_surfaces",
)


def fail(message: str) -> NoReturn:
    print(f"OFFENSIVE_EVIDENCE_VALIDATOR_ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


def sha256(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def no_duplicate_object_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key!r}")
        result[key] = value
    return result


def atomic_json(path: Path, payload: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_symlink():
        fail(f"output verdict is a symlink: {path}")
    temp = path.with_name(f".{path.name}.tmp.{os.getpid()}")
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


def invalidate_previous_verdict(path: Path) -> str | None:
    """Try to remove one prior verdict, returning an error instead of ending the sweep."""
    try:
        mode = path.lstat().st_mode
    except FileNotFoundError:
        return None
    except OSError as exc:
        return f"cannot stat prior verdict output {path}: {exc}"
    if stat.S_ISDIR(mode):
        return f"verdict output must not be a directory: {path}"
    try:
        path.unlink()
    except OSError as exc:
        return f"cannot invalidate prior verdict output {path}: {exc}"
    return None


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(allow_abbrev=False)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--expected-binary-sha256", required=True)
    parser.add_argument("--expected-memory-mib", required=True, type=int)
    parser.add_argument("--expected-jobs", required=True, type=int)
    return parser


def parser_accepts_separate_value(parser: argparse.ArgumentParser, value: str) -> bool:
    """Use argparse's own classifier without performing its exit-capable authoritative parse."""
    if value == "--":
        return False
    # _parse_optional is the classifier parse_args itself uses to distinguish an option token from
    # a positional/value token. Keeping this private call isolated here avoids maintaining a subtly
    # different grammar for negative numbers, spaces, registered options, and unknown options.
    return parser._parse_optional(value) is None  # type: ignore[attr-defined]


def requested_verdict_paths(
    argv: list[str], parser: argparse.ArgumentParser
) -> tuple[list[Path], list[str]]:
    """Collect every exact --out target and any raw-syntax errors without parsing."""
    paths: list[Path] = []
    errors: list[str] = []
    index = 0
    while index < len(argv):
        token = argv[index]
        if token == "--":
            break
        if token.startswith("--out="):
            value = token.removeprefix("--out=")
            if value:
                paths.append(Path(value))
            else:
                errors.append("--out requires a non-empty path")
            index += 1
            continue
        if token == "--out":
            if index + 1 < len(argv):
                value = argv[index + 1]
                if not value:
                    errors.append("--out requires a non-empty path")
                    index += 2
                    continue
                if parser_accepts_separate_value(parser, value):
                    paths.append(Path(value))
                    index += 2
                    continue
            index += 1
            continue
        index += 1
    return paths, errors


def invalidate_requested_verdicts(paths: list[Path]) -> None:
    """Attempt every target before refusing the invocation for any invalidation error."""
    errors: list[str] = []
    for path in paths:
        error = invalidate_previous_verdict(path)
        if error is not None:
            errors.append(error)
    if errors:
        fail("; ".join(errors))


def load_json(path: Path, errors: list[str]) -> tuple[Any | None, bytes | None]:
    try:
        mode = path.lstat().st_mode
    except OSError as exc:
        errors.append(f"missing required file {path.name}: {exc}")
        return None, None
    if stat.S_ISLNK(mode):
        errors.append(f"required file is a symlink: {path.name}")
        return None, None
    if not stat.S_ISREG(mode):
        errors.append(f"required file is not regular: {path.name}")
        return None, None
    try:
        raw = path.read_bytes()
        value = json.loads(raw.decode("utf-8"), object_pairs_hook=no_duplicate_object_pairs)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as exc:
        errors.append(f"invalid JSON in {path.name}: {exc}")
        return None, None
    if not isinstance(value, dict):
        errors.append(f"JSON root is not an object: {path.name}")
        return None, raw
    return value, raw


def main() -> int:
    parser = build_parser()
    # Discover every verdict target without invoking argparse's exit-capable parse: even
    # parse_known_args() can exit on a dangling final --out, and it retains only the last repeated
    # target. Invalidate all resolved targets first; the authoritative parser below still owns
    # normal help, missing-value, and type errors.
    output_paths, output_syntax_errors = requested_verdict_paths(sys.argv[1:], parser)
    invalidate_requested_verdicts(output_paths)
    if output_syntax_errors:
        fail("; ".join(output_syntax_errors))

    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{64}", args.expected_binary_sha256):
        fail("expected binary SHA-256 must be 64 lowercase hex")
    if args.expected_memory_mib <= 0 or args.expected_jobs <= 0:
        fail("expected memory/jobs must be positive")
    evidence = args.evidence.absolute()
    try:
        mode = evidence.lstat().st_mode
    except OSError as exc:
        fail(f"cannot stat evidence directory: {exc}")
    if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
        fail(f"evidence must be a real directory: {evidence}")

    errors: list[str] = []
    for directory, dirnames, filenames in os.walk(evidence, followlinks=False):
        current = Path(directory)
        for name in [*dirnames, *filenames]:
            path = current / name
            try:
                if path.is_symlink():
                    errors.append(f"symlink present in evidence tree: {path.relative_to(evidence)}")
            except OSError as exc:
                errors.append(f"cannot inspect evidence path {path}: {exc}")

    report, report_raw = load_json(evidence / "report.json", errors)
    isolation, isolation_raw = load_json(evidence / "isolation.json", errors)
    export, export_raw = load_json(evidence / "export_manifest.json", errors)

    if report is not None:
        expected_report = {
            "total": 34,
            "passed": 34,
            "failed": 0,
            "overall_verdict": "PASS",
            "expected_total": 34,
            "isolation": "tart-disposable-guest",
            "mode": "tart-disposable-guest",
            "binary_sha256": args.expected_binary_sha256,
            "teardown_status": "torn_down",
        }
        for key, wanted in expected_report.items():
            if report.get(key) != wanted:
                errors.append(f"report field {key!r}={report.get(key)!r}, expected {wanted!r}")
        cases = report.get("cases")
        if not isinstance(cases, list):
            errors.append("report case roster is missing or not a list")
        else:
            names: list[str] = []
            statuses: list[object] = []
            for index, row in enumerate(cases):
                if not isinstance(row, dict):
                    errors.append(f"report case roster row {index} is not an object")
                    continue
                names.append(str(row.get("name")))
                statuses.append(row.get("status"))
            if tuple(names) != EXPECTED_CASES:
                errors.append(f"report case roster mismatch: observed={names!r}")
            bad_statuses = [(name, status) for name, status in zip(names, statuses) if status != "PASS"]
            if bad_statuses:
                errors.append(f"report case roster has non-PASS cases: {bad_statuses!r}")
    if isolation is not None:
        expected_isolation = {
            "isolation": "tart-disposable-guest",
            "mode": "tart-disposable-guest",
            "cpu": 8,
            "memory_mib": args.expected_memory_mib,
            "cargo_build_jobs": args.expected_jobs,
            "rayon_threads": args.expected_jobs,
            "binary_sha256": args.expected_binary_sha256,
            "teardown_status": "torn_down",
        }
        for key, wanted in expected_isolation.items():
            if isolation.get(key) != wanted:
                label = "jobs" if key in {"cargo_build_jobs", "rayon_threads"} else key
                errors.append(f"isolation {label} field {key!r}={isolation.get(key)!r}, expected {wanted!r}")
        guest = isolation.get("guest")
        if not isinstance(guest, str) or not re.fullmatch(r"anubis-offensive-gate-[0-9]+", guest):
            errors.append(f"isolation guest name is invalid: {guest!r}")

    teardown = evidence / "teardown_status.txt"
    try:
        teardown_mode = teardown.lstat().st_mode
        if stat.S_ISLNK(teardown_mode) or not stat.S_ISREG(teardown_mode):
            errors.append("teardown_status.txt is not a regular non-symlink file")
        elif teardown.read_text(encoding="utf-8") != "torn_down\n":
            errors.append("teardown_status.txt is not exact torn_down")
    except (OSError, UnicodeDecodeError) as exc:
        errors.append(f"cannot read teardown_status.txt: {exc}")

    guest_log = evidence / "guest_stdout.log"
    try:
        guest_mode = guest_log.lstat().st_mode
        if stat.S_ISLNK(guest_mode) or not stat.S_ISREG(guest_mode):
            errors.append("guest_stdout.log is not a regular non-symlink file")
        else:
            lines = guest_log.read_text(encoding="utf-8").splitlines()
            overall = [line for line in lines if line.startswith("Overall:")]
            expected_overall = "Overall: PASS (34/34) isolation=tart-disposable-guest expected=34"
            if overall != [expected_overall]:
                errors.append(
                    f"guest Overall markers={overall!r}, expected exactly one final producer PASS line"
                )
    except (OSError, UnicodeDecodeError) as exc:
        errors.append(f"cannot read guest_stdout.log: {exc}")

    if export is not None:
        if export.get("schema") != "anubis-offensive-gate-export-v1":
            errors.append(f"export schema is invalid: {export.get('schema')!r}")
        if export.get("secret_scan") != "PASS":
            errors.append(f"export secret_scan is not PASS: {export.get('secret_scan')!r}")
        files = export.get("files")
        if not isinstance(files, list) or len(files) != 1 or not isinstance(files[0], dict):
            errors.append("export files must contain exactly one report.json row")
        elif report_raw is not None:
            row = files[0]
            if row.get("path") != "report.json":
                errors.append(f"export path is not report.json: {row.get('path')!r}")
            if row.get("size_bytes") != len(report_raw):
                errors.append("export report size does not match actual report")
            if row.get("sha256") != sha256(report_raw):
                errors.append("export report sha256 does not match actual report")

    hashes: dict[str, dict[str, object]] = {}
    for name in REQUIRED:
        path = evidence / name
        if path.is_file() and not path.is_symlink():
            raw = path.read_bytes()
            hashes[name] = {"bytes": len(raw), "sha256": sha256(raw)}
    payload: dict[str, object] = {
        "schema": "anubis.offensive-evidence-verdict.v1",
        "verdict": "PASS" if not errors else "FAIL",
        "expected_binary_sha256": args.expected_binary_sha256,
        "expected_memory_mib": args.expected_memory_mib,
        "expected_jobs": args.expected_jobs,
        "files": hashes,
        "errors": errors,
    }
    atomic_json(args.out, payload)
    if errors:
        print(f"OFFENSIVE_EVIDENCE_VALIDATE_FAIL errors={len(errors)} verdict={args.out}")
        return 1
    print(f"OFFENSIVE_EVIDENCE_VALIDATE_PASS verdict={args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
