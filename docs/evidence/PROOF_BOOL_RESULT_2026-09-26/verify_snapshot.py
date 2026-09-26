#!/usr/bin/env python3
"""Recheck recorded hashes against immutable Git source and retained local test bytes."""
import argparse
import hashlib
import json
import pathlib
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--check", action="store_true", required=True)
parser.add_argument("--repo", type=pathlib.Path, default=pathlib.Path.cwd())
parser.add_argument("--source-only", action="store_true")
args = parser.parse_args()
manifest = json.loads(pathlib.Path(__file__).with_name("source-manifest.json").read_text())
process = subprocess.Popen(["git", "cat-file", "--batch"], cwd=args.repo,
                           stdin=subprocess.PIPE, stdout=subprocess.PIPE)
try:
    for name, expected in manifest["files"].items():
        process.stdin.write(f"{manifest['source_commit']}:{name}\n".encode())
        process.stdin.flush()
        header = process.stdout.readline().split()
        if len(header) != 3 or header[1] != b"blob":
            raise RuntimeError(f"missing source blob: {name}")
        data = process.stdout.read(int(header[2]))
        if process.stdout.read(1) != b"\n":
            raise RuntimeError(f"truncated source blob: {name}")
        actual = ({"symlink": data.decode()} if "symlink" in expected
                  else {"sha256": hashlib.sha256(data).hexdigest()})
        if actual != expected:
            raise RuntimeError(f"source hash mismatch: {name}")
finally:
    process.stdin.close()
    process.stdout.close()
    process.wait()
if process.returncode:
    raise SystemExit(process.returncode)
if not args.source_only:
    for artifact in manifest["artifacts"]:
        path = pathlib.Path(artifact["path"])
        if hashlib.sha256(path.read_bytes()).hexdigest() != artifact["sha256"]:
            raise RuntimeError(f"retained test executable mismatch: {path}")
print("source snapshot hashes match; " +
      ("artifact check explicitly skipped" if args.source_only else "retained test executable matches"))
