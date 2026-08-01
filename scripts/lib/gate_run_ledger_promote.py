#!/usr/bin/env python3
"""Publish one verdict-bound gate-run ledger without following or replacing paths."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import secrets
import stat
import sys
from pathlib import Path
from typing import NoReturn


def fail(message: str) -> NoReturn:
    print(f"GATE_RUN_LEDGER_PROMOTE_ERROR: {message}", file=sys.stderr)
    raise SystemExit(2)


def identity(metadata: os.stat_result) -> tuple[int, int, int, int, int, int, int]:
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
        metadata.st_mode,
        metadata.st_nlink,
    )


def read_source(path: Path) -> tuple[bytes, os.stat_result]:
    if not hasattr(os, "O_NOFOLLOW"):
        fail("platform lacks O_NOFOLLOW for ledger reads")
    try:
        before = path.lstat()
    except OSError as exc:
        fail(f"cannot stat working ledger: {exc}")
    if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
        fail(f"working ledger must be a regular non-symlink file: {path}")
    if before.st_mode & 0o222:
        fail(f"working ledger must be non-writable: {path}")
    flags = os.O_RDONLY | os.O_NOFOLLOW
    chunks: list[bytes] = []
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb") as handle:
            opened = os.fstat(handle.fileno())
            if not stat.S_ISREG(opened.st_mode):
                fail("working ledger changed type while opening")
            while chunk := handle.read(1024 * 1024):
                chunks.append(chunk)
            after = os.fstat(handle.fileno())
    except OSError as exc:
        fail(f"cannot read working ledger: {exc}")
    if identity(before) != identity(opened) or identity(opened) != identity(after):
        fail("working ledger changed while reading")
    return b"".join(chunks), before


def ledger_commit(raw: bytes) -> tuple[str, int]:
    try:
        text = raw.decode("ascii")
    except UnicodeDecodeError:
        fail("working ledger is not ASCII")
    if not text.endswith("\n") or "\r" in text:
        fail("working ledger is not canonical LF-delimited text")
    commits: set[str] = set()
    rows = text.splitlines()
    if not rows:
        fail("working ledger is empty")
    for index, row in enumerate(rows, start=1):
        fields = row.split(" ")
        if (
            len(fields) != 3
            or not fields[0]
            or re.fullmatch(r"[0-9a-f]{40}", fields[1]) is None
            or re.fullmatch(r"[0-9]+", fields[2]) is None
        ):
            fail(f"malformed working ledger row {index}")
        commits.add(fields[1])
    if len(commits) != 1:
        fail(f"working ledger has mixed commit epochs: {sorted(commits)!r}")
    return next(iter(commits)), len(rows)


def write_all(descriptor: int, payload: bytes) -> None:
    offset = 0
    while offset < len(payload):
        written = os.write(descriptor, payload[offset:])
        if written <= 0:
            fail("short write while snapshotting ledger")
        offset += written


def promote(source: Path, destination: Path, expected_sha: str, expected_commit: str) -> dict[str, object]:
    if not hasattr(os, "O_NOFOLLOW"):
        fail("platform lacks O_NOFOLLOW for ledger promotion")
    if not source.is_absolute() or not destination.is_absolute():
        fail("source and destination must be absolute paths")
    if source.parent != destination.parent or source == destination:
        fail("source and destination must be distinct names in one directory")
    try:
        parent_mode = source.parent.lstat().st_mode
    except OSError as exc:
        fail(f"cannot stat ledger directory: {exc}")
    if stat.S_ISLNK(parent_mode) or not stat.S_ISDIR(parent_mode):
        fail(f"ledger parent must be a real directory: {source.parent}")
    if re.fullmatch(r"[0-9a-f]{64}", expected_sha) is None:
        fail("expected SHA-256 is malformed")
    if re.fullmatch(r"[0-9a-f]{40}", expected_commit) is None:
        fail("expected commit is malformed")
    try:
        destination.lstat()
    except FileNotFoundError:
        pass
    except OSError as exc:
        fail(f"cannot inspect destination: {exc}")
    else:
        fail(f"destination already exists: {destination}")

    raw, source_metadata = read_source(source)
    actual_sha = hashlib.sha256(raw).hexdigest()
    actual_commit, rows = ledger_commit(raw)
    if actual_sha != expected_sha:
        fail(f"working ledger digest mismatch: {actual_sha} != {expected_sha}")
    if actual_commit != expected_commit:
        fail(f"working ledger commit mismatch: {actual_commit} != {expected_commit}")

    temporary = source.parent / f".gate-run-ledger.promote.{os.getpid()}.{secrets.token_hex(8)}"
    descriptor = -1
    linked = False
    try:
        flags = os.O_RDWR | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
        descriptor = os.open(temporary, flags, 0o444)
        write_all(descriptor, raw)
        os.fchmod(descriptor, 0o444)
        os.fsync(descriptor)
        snapshot_metadata = os.fstat(descriptor)
        if not stat.S_ISREG(snapshot_metadata.st_mode) or snapshot_metadata.st_mode & 0o222:
            fail("ledger snapshot is not a non-writable regular file")
        try:
            current_source = source.lstat()
        except OSError as exc:
            fail(f"working ledger path changed before promotion: {exc}")
        if identity(current_source) != identity(source_metadata):
            fail("working ledger path changed before promotion")

        # Hard-link publication is same-filesystem, atomic, and refuses an existing
        # destination instead of overwriting it. The linked inode is our private,
        # no-follow snapshot—not the mutable source pathname.
        os.link(temporary, destination, follow_symlinks=False)
        linked = True
        published = destination.lstat()
        snapshot_after_link = os.fstat(descriptor)
        if (
            stat.S_ISLNK(published.st_mode)
            or not stat.S_ISREG(published.st_mode)
            or published.st_mode & 0o222
            or published.st_nlink != 2
            or published.st_dev != snapshot_metadata.st_dev
            or published.st_ino != snapshot_metadata.st_ino
            or identity(published) != identity(snapshot_after_link)
        ):
            fail("published ledger identity is not the validated snapshot")
        os.lseek(descriptor, 0, os.SEEK_SET)
        published_chunks: list[bytes] = []
        while chunk := os.read(descriptor, 1024 * 1024):
            published_chunks.append(chunk)
        snapshot_after_read = os.fstat(descriptor)
        if b"".join(published_chunks) != raw or identity(snapshot_after_read) != identity(snapshot_after_link):
            fail("published ledger bytes changed during atomic promotion")
        temporary.unlink()
        published_final = destination.lstat()
        snapshot_final = os.fstat(descriptor)
        if (
            stat.S_ISLNK(published_final.st_mode)
            or not stat.S_ISREG(published_final.st_mode)
            or published_final.st_mode & 0o222
            or published_final.st_nlink != 1
            or identity(published_final) != identity(snapshot_final)
        ):
            fail("published ledger did not close to one immutable destination link")
        os.close(descriptor)
        descriptor = -1
        os.unlink(source)
        try:
            source.lstat()
        except FileNotFoundError:
            pass
        else:
            fail("working ledger still exists after promotion")
        return {
            "commit": actual_commit,
            "destination": str(destination),
            "rows": rows,
            "sha256": actual_sha,
        }
    except FileExistsError:
        fail(f"promotion destination or temporary path already exists: {destination}")
    except OSError as exc:
        fail(f"atomic ledger promotion failed: {exc}")
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass
        if linked:
            # The destination is intentionally retained. Any later validation
            # failure makes the seal REFUSED rather than exposing a false PASS.
            pass


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    parser.add_argument("--expected-sha256", required=True)
    parser.add_argument("--expected-commit", required=True)
    args = parser.parse_args()
    result = promote(
        args.source,
        args.destination,
        args.expected_sha256,
        args.expected_commit,
    )
    json.dump(result, sys.stdout, sort_keys=True, separators=(",", ":"))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
