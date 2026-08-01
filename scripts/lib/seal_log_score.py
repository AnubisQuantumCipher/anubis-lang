#!/usr/bin/env python3
"""Score one gate log by exact declared verdict markers."""
from __future__ import annotations

import argparse
import json
import os
import re
import stat
import sys
from pathlib import Path
from typing import NoReturn


def fail(message: str) -> NoReturn:
    print(f"SEAL_LOG_SCORE_ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


def atomic_write(path: Path, payload: dict[str, object]) -> None:
    if path.is_symlink():
        fail(f"score output is symlink: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.tmp.{os.getpid()}")
    data = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    try:
        with tmp.open("x", encoding="utf-8") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    finally:
        try:
            tmp.unlink()
        except FileNotFoundError:
            pass


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--log", required=True, type=Path)
    parser.add_argument("--pass-re", required=True)
    parser.add_argument("--fail-re", required=True)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    try:
        mode = args.log.lstat().st_mode
    except OSError as exc:
        fail(f"cannot stat log: {exc}")
    if stat.S_ISLNK(mode) or not stat.S_ISREG(mode):
        fail(f"log must be a regular non-symlink file: {args.log}")
    try:
        pass_re = re.compile(args.pass_re)
        fail_re = re.compile(args.fail_re)
    except re.error as exc:
        fail(f"invalid verdict regex: {exc}")
    pass_hits: list[tuple[int, str]] = []
    fail_hits: list[tuple[int, str]] = []
    try:
        with args.log.open(encoding="utf-8", errors="replace") as handle:
            for number, raw in enumerate(handle, 1):
                line = raw.rstrip("\n")
                if pass_re.search(line):
                    pass_hits.append((number, line))
                if fail_re.search(line):
                    fail_hits.append((number, line))
    except OSError as exc:
        fail(f"cannot read log: {exc}")
    declared_marker_count = len(pass_hits) + len(fail_hits)
    reason = ""
    status = "FAIL"
    line = ""
    if declared_marker_count == 0:
        reason = "no_declared_verdict_line"
    elif len(pass_hits) == 1 and not fail_hits:
        status = "PASS"
        reason = "declared_PASS_line"
        line = pass_hits[0][1]
    elif len(fail_hits) == 1 and not pass_hits:
        reason = "declared_FAIL_line"
        line = fail_hits[0][1]
    elif len(pass_hits) > 1 or len(fail_hits) > 1:
        reason = f"duplicate_declared_verdict_marker_count={declared_marker_count}"
    else:
        reason = f"contradictory_declared_verdict_markers pass={len(pass_hits)} fail={len(fail_hits)}"
    payload: dict[str, object] = {
        "schema": "anubis.seal-log-score.v1",
        "status": status,
        "reason": reason,
        "declared_verdict_line": line,
        "declared_marker_count": declared_marker_count,
        "pass_marker_count": len(pass_hits),
        "fail_marker_count": len(fail_hits),
        "pass_lines": [number for number, _line in pass_hits],
        "fail_lines": [number for number, _line in fail_hits],
    }
    atomic_write(args.out, payload)
    print(f"SEAL_LOG_SCORE status={status} reason={reason} declared_marker_count={declared_marker_count}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
