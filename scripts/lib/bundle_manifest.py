#!/usr/bin/env python3
"""Strict, atomic evidence-bundle manifest producer."""
from __future__ import annotations

import argparse
import hashlib
import os
import stat
import sys
from pathlib import Path
from typing import NoReturn

MANIFEST = "MANIFEST.sha256"


def fail(message: str) -> NoReturn:
    print(f"BUNDLE_MANIFEST_ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def hash_regular(path: Path) -> str:
    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as exc:
        fail(f"cannot open regular non-symlink member {path}: {exc}")
    digest = hashlib.sha256()
    try:
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode):
            fail(f"bundle member is not regular: {path}")
        with os.fdopen(descriptor, "rb", closefd=False) as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                digest.update(chunk)
        after = os.fstat(descriptor)
        identity_before = (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
        identity_after = (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns)
        if identity_before != identity_after:
            fail(f"bundle member changed while hashing: {path}")
    finally:
        os.close(descriptor)
    return digest.hexdigest()


def collect(bundle: Path) -> list[str]:
    try:
        mode = bundle.lstat().st_mode
    except OSError as exc:
        fail(f"cannot stat bundle {bundle}: {exc}")
    if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
        fail(f"bundle must be a real directory: {bundle}")

    paths: list[str] = []

    def onerror(exc: OSError) -> None:
        fail(f"bundle traversal failed: {exc}")

    for directory, dirnames, filenames in os.walk(bundle, followlinks=False, onerror=onerror):
        current = Path(directory)
        for dirname in list(dirnames):
            child = current / dirname
            try:
                child_mode = child.lstat().st_mode
            except OSError as exc:
                fail(f"cannot stat bundle directory {child}: {exc}")
            if stat.S_ISLNK(child_mode):
                fail(f"symlink directory in bundle: {child.relative_to(bundle)}")
        dirnames.sort()
        filenames.sort()
        for filename in filenames:
            path = current / filename
            rel = path.relative_to(bundle).as_posix()
            try:
                file_mode = path.lstat().st_mode
            except OSError as exc:
                fail(f"cannot stat bundle member {rel}: {exc}")
            if stat.S_ISLNK(file_mode) or not stat.S_ISREG(file_mode):
                fail(f"bundle member must be regular and non-symlink: {rel}")
            if rel == MANIFEST:
                continue
            paths.append(rel)
    paths.sort()
    if not paths:
        fail("bundle has no manifestable members")
    if len(paths) != len(set(paths)):
        fail("bundle member list contains duplicates")
    return paths


def rehash(bundle: Path) -> int:
    bundle = bundle.absolute()
    paths = collect(bundle)
    rows = [f"{hash_regular(bundle / rel)}  {rel}\n" for rel in paths]
    manifest = bundle / MANIFEST
    temp = bundle / f".{MANIFEST}.tmp.{os.getpid()}"
    if temp.exists() or temp.is_symlink():
        fail(f"temporary manifest path already exists: {temp}")
    try:
        descriptor = os.open(temp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w", encoding="utf-8") as handle:
            handle.writelines(rows)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp, manifest)
    except OSError as exc:
        fail(f"cannot publish manifest atomically: {exc}")
    finally:
        try:
            temp.unlink()
        except FileNotFoundError:
            pass
    print(f"BUNDLE_MANIFEST_REHASH_PASS files={len(rows)} path={manifest}")
    return 0


def verify(bundle: Path) -> int:
    """Verify both the exact member roster and every byte digest without rewriting evidence."""
    bundle = bundle.absolute()
    paths = collect(bundle)
    manifest = bundle / MANIFEST
    try:
        mode = manifest.lstat().st_mode
    except OSError as exc:
        fail(f"cannot stat manifest {manifest}: {exc}")
    if stat.S_ISLNK(mode) or not stat.S_ISREG(mode):
        fail(f"manifest must be a regular non-symlink file: {manifest}")
    expected = [f"{hash_regular(bundle / rel)}  {rel}\n" for rel in paths]
    try:
        actual = manifest.read_text(encoding="utf-8").splitlines(keepends=True)
    except (OSError, UnicodeDecodeError) as exc:
        fail(f"cannot read manifest {manifest}: {exc}")
    if actual != expected:
        fail("manifest rows do not exactly match the current sorted bundle roster and digests")
    print(f"BUNDLE_MANIFEST_VERIFY_PASS files={len(expected)} path={manifest}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    for command_name in ("rehash", "verify"):
        command = subparsers.add_parser(command_name)
        command.add_argument("--bundle", required=True, type=Path)
    args = parser.parse_args()
    if args.command == "rehash":
        return rehash(args.bundle)
    if args.command == "verify":
        return verify(args.bundle)
    fail(f"unknown command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
