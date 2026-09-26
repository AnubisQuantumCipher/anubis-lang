#!/usr/bin/env python3
"""Emit bounded, public-safe identifiers for a failed hosted CI run.

This is a diagnostic, not a gate receipt. In particular, cargo test output is
untrusted text: until a reviewed public-source test-ID catalog exists, G3 has
no publishable test names. Raw logs and diagnostic messages never leave CI.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
GATE_ROOT = ROOT / "out/ci_gate"
PUBLIC_FILE = ROOT / "out/ci_failure_public/failure_ids.json"
FIXTURE_DIR = ROOT / "tests/fixtures/language_core"
FIXTURE_PREFIX = "tests/fixtures/language_core/"
FLOOR_NAME = FIXTURE_PREFIX + ".fixture_count_floor"
FIXTURE_ID = re.compile(r"[a-z][a-z0-9_]*\Z")
COMMIT_ID = re.compile(r"[0-9a-f]{40}\Z")
RUN_ID = re.compile(r"[1-9][0-9]{0,19}\Z")
FLOOR_VALUE = re.compile(r"(?:0|[1-9][0-9]*)\Z")
MAX_INPUT_BYTES = 1_048_576
MAX_PUBLIC_IDS = 32


def unavailable(reason: str) -> dict[str, str]:
    """Only call with an in-source reason enum, never an input string."""
    return {"status": "unavailable", "reason": reason}


def read_bounded(path: Path, limit: int) -> tuple[bytes | None, str | None]:
    """Read one regular file without following a symlink or exposing its path."""
    try:
        if not stat.S_ISREG(path.lstat().st_mode):
            return None, "invalid"
        # A path can be replaced between lstat and open. NONBLOCK makes a raced-in
        # FIFO harmless; fstat still rejects every non-regular opened object.
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | os.O_NONBLOCK
        with os.fdopen(os.open(path, flags), "rb") as source:
            if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
                return None, "invalid"
            contents = source.read(limit + 1)
    except FileNotFoundError:
        return None, "missing"
    except OSError:
        return None, "invalid"
    if len(contents) > limit:
        return None, "invalid"
    return contents, None


def read_json(path: Path) -> tuple[object | None, str | None]:
    contents, error = read_bounded(path, MAX_INPUT_BYTES)
    if error:
        return None, error
    try:
        def unique_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
            result: dict[str, object] = {}
            for key, value in pairs:
                if key in result:
                    raise ValueError("duplicate JSON key")
                result[key] = value
            return result

        return json.loads(contents.decode("utf-8"), object_pairs_hook=unique_keys), None
    except (UnicodeError, ValueError, RecursionError):
        return None, "invalid"


def target_gate_statuses(report: object) -> dict[str, str] | None:
    """Read only the fixed G3/G5 status fields; discard all free-form details."""
    if not isinstance(report, dict):
        return None
    if report.get("profile") != "hosted" or report.get("verdict") not in ("FAIL", "HOSTED_PASS"):
        return None
    rows = report.get("gates")
    if not isinstance(rows, list):
        return None
    targets = {"G3_test", "G5_language_fixtures"}
    result: dict[str, str] = {}
    for row in rows:
        if not isinstance(row, dict):
            return None
        name = row.get("gate")
        if not isinstance(name, str):
            return None
        if name not in targets:
            continue
        status = row.get("status")
        if name in result or status not in ("PASS", "FAIL"):
            return None
        result[name] = status
    if set(result) != targets:
        return None
    if report["verdict"] == "HOSTED_PASS" and any(status != "PASS" for status in result.values()):
        return None
    return result


def fixture_catalog() -> tuple[set[str] | None, int | None, str | None]:
    """Build a name allowlist from the Git index, never from gate output."""
    try:
        listed = subprocess.run(
            ["git", "ls-files", "-z", "--", FIXTURE_PREFIX],
            cwd=ROOT,
            check=True,
            capture_output=True,
            timeout=10,
        ).stdout
        names = {entry.decode("utf-8") for entry in listed.split(b"\0") if entry}
    except (OSError, UnicodeError, subprocess.SubprocessError):
        return None, None, "fixture_catalog_unavailable"
    if FLOOR_NAME not in names:
        return None, None, "fixture_floor_missing"
    floor_raw, floor_error = read_bounded(FIXTURE_DIR / ".fixture_count_floor", 64)
    if floor_error:
        return None, None, "fixture_floor_missing" if floor_error == "missing" else "fixture_floor_invalid"
    try:
        floor_text = floor_raw.decode("ascii").rstrip("\n")
    except UnicodeError:
        return None, None, "fixture_floor_invalid"
    if not FLOOR_VALUE.fullmatch(floor_text):
        return None, None, "fixture_floor_invalid"

    allowed: set[str] = set()
    for path in names:
        if not path.startswith(FIXTURE_PREFIX) or not path.endswith(".anb"):
            continue
        basename = path[len(FIXTURE_PREFIX) : -len(".anb")]
        if not FIXTURE_ID.fullmatch(basename) or basename in allowed:
            return None, None, "fixture_catalog_unavailable"
        candidate = FIXTURE_DIR / (basename + ".anb")
        try:
            mode = candidate.lstat().st_mode
        except OSError:
            return None, None, "fixture_catalog_unavailable"
        if not stat.S_ISREG(mode):
            return None, None, "fixture_catalog_unavailable"
        allowed.add(basename)
    if not allowed:
        return None, None, "fixture_catalog_unavailable"
    return allowed, int(floor_text), None


def summarize_g5(report: object, allowed: set[str], floor: int) -> dict[str, object]:
    """Publish only complete, internally consistent rows with tracked IDs."""
    if not isinstance(report, dict):
        return unavailable("invalid_fixture_report")
    if report.get("overall_verdict") != "FAIL":
        return unavailable("inconsistent_fixture_report")
    rows = report.get("fixtures")
    counts = [report.get(key) for key in ("total", "passed", "failed")]
    if not isinstance(rows, list) or any(type(value) is not int or value < 0 for value in counts):
        return unavailable("invalid_fixture_report")
    total, passed, failed = counts
    if passed + failed != total or len(rows) != total or total != len(allowed):
        return unavailable("incomplete_fixture_report")

    seen: set[str] = set()
    failed_ids: list[str] = []
    passed_rows = 0
    failed_rows = 0
    for row in rows:
        if not isinstance(row, dict):
            return unavailable("invalid_fixture_report")
        name = row.get("name")
        expected = row.get("expected")
        actual = row.get("actual")
        status = row.get("status")
        if (
            not isinstance(name, str)
            or not FIXTURE_ID.fullmatch(name)
            or name not in allowed
            or name in seen
            or expected not in ("PASS", "FAIL")
            or actual not in ("PASS", "FAIL", "UNKNOWN")
            or status not in ("PASS", "FAIL")
            or (status == "PASS" and expected != actual)
        ):
            return unavailable("invalid_fixture_report")
        seen.add(name)
        if status == "FAIL":
            failed_rows += 1
            failed_ids.append(name)
        else:
            passed_rows += 1
    if seen != allowed or passed_rows != passed or failed_rows != failed:
        return unavailable("incomplete_fixture_report")
    if not failed_ids:
        return unavailable("corpus_floor" if total < floor else "no_failed_fixture_rows")

    failed_ids.sort()
    truncated = len(failed_ids) > MAX_PUBLIC_IDS
    return {
        "status": "partial" if truncated else "identified",
        "fixture_ids": failed_ids[:MAX_PUBLIC_IDS],
        "truncated": truncated,
        "corpus_floor": total < floor,
    }


def build_diagnostic(
    gate_report: object | None,
    fixture_report: object | None,
    allowed: set[str] | None,
    floor: int | None,
    *,
    gate_error: str | None = None,
    fixture_error: str | None = None,
    catalog_error: str | None = None,
) -> dict[str, object]:
    if gate_error:
        reason = "missing_gate_report" if gate_error == "missing" else "invalid_gate_report"
        return {"g3_test": unavailable(reason), "g5_language_fixtures": unavailable(reason)}
    statuses = target_gate_statuses(gate_report)
    if statuses is None:
        return {
            "g3_test": unavailable("invalid_gate_report"),
            "g5_language_fixtures": unavailable("invalid_gate_report"),
        }
    g3 = (
        unavailable("no_reviewed_public_test_id_allowlist")
        if statuses["G3_test"] == "FAIL"
        else {"status": "not_failed"}
    )
    if statuses["G5_language_fixtures"] != "FAIL":
        g5: dict[str, object] = {"status": "not_failed"}
    elif fixture_error:
        reason = "missing_fixture_report" if fixture_error == "missing" else "invalid_fixture_report"
        g5 = unavailable(reason)
    elif catalog_error:
        g5 = unavailable(catalog_error)
    elif allowed is None or floor is None:
        g5 = unavailable("fixture_catalog_unavailable")
    else:
        g5 = summarize_g5(fixture_report, allowed, floor)
    return {"g3_test": g3, "g5_language_fixtures": g5}


def main() -> int:
    sha = os.environ.get("GITHUB_SHA", "")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    if not COMMIT_ID.fullmatch(sha) or not RUN_ID.fullmatch(run_id):
        return 1
    try:
        actual_sha = subprocess.run(
            ["git", "rev-parse", "--verify", "HEAD^{commit}"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
            timeout=10,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return 1
    if actual_sha != sha:
        return 1

    gate_report, gate_error = read_json(GATE_ROOT / "gate_report.json")
    fixture_report, fixture_error = read_json(GATE_ROOT / "g5_language/fixture_report.json")
    allowed, floor, catalog_error = fixture_catalog()
    document = {
        "schema": "anubis.hosted-failure-ids.v1",
        "role": "diagnostic_only",
        "source_commit": sha,
        "run_id": run_id,
        **build_diagnostic(
            gate_report,
            fixture_report,
            allowed,
            floor,
            gate_error=gate_error,
            fixture_error=fixture_error,
            catalog_error=catalog_error,
        ),
    }
    try:
        PUBLIC_FILE.parent.mkdir(parents=True, exist_ok=True)
        if PUBLIC_FILE.parent.is_symlink():
            return 1
        with PUBLIC_FILE.open("x", encoding="utf-8") as output:
            json.dump(document, output, sort_keys=True, separators=(",", ":"))
            output.write("\n")
    except OSError:
        return 1
    print("Hosted failure identifier artifact ready (diagnostic only).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
