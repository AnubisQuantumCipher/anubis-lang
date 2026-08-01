#!/usr/bin/env python3
"""Read one exact lowercase SHA-256 line from a regular, non-symlink file."""

from __future__ import annotations

import os
import re
import stat
import sys
from pathlib import Path


EXACT_SHA256_FILE = re.compile(rb"[0-9a-f]{64}\n\Z")


def read_exact_sha256(path: Path) -> str:
    flags = (
        os.O_RDONLY
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_NONBLOCK", 0)
    )
    fd = os.open(path, flags)
    try:
        opened = os.fstat(fd)
        if not stat.S_ISREG(opened.st_mode):
            raise ValueError("not a regular file")
        data = os.read(fd, 66)
        if len(data) == 66 or os.read(fd, 1):
            raise ValueError("not exactly 65 bytes")
    finally:
        os.close(fd)
    if EXACT_SHA256_FILE.fullmatch(data) is None:
        raise ValueError("not exactly one lowercase SHA-256 line")
    return data[:64].decode("ascii")


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: read_exact_sha256.py PATH", file=sys.stderr)
        return 2
    try:
        digest = read_exact_sha256(Path(argv[1]))
    except (OSError, ValueError) as exc:
        print(f"READ_EXACT_SHA256: REFUSED ({exc})", file=sys.stderr)
        return 2
    print(digest)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
