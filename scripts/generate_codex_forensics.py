#!/usr/bin/env python3
"""Create a byte-level census of the entire Anubis checkout.

The scanner snapshots paths before opening its output directory, so its own
generated artifacts do not recursively enter the census. Every regular file in
that snapshot is read to EOF and SHA-256 hashed. Text files also receive exact
line counts. Build products, VCS internals, workflow state, and audit evidence
remain in the whole-tree inventory, but are separated from the authoritative
source/document/config corpus.
"""

from __future__ import annotations

import argparse
import collections
import datetime as dt
import gzip
import hashlib
import io
import json
import os
import re
import stat
import sys
from pathlib import Path
from typing import BinaryIO, Iterable, TextIO


GENERATED_TOP_LEVEL = {".git", ".claude", "target", "out", "implementer"}
MARKERS = {
    "unsafe": re.compile(r"\bunsafe\b"),
    "kani_proof": re.compile(r"#\s*\[\s*kani::proof\s*\]"),
    "test": re.compile(r"#\s*\[\s*test\s*\]"),
    "tokio_test": re.compile(r"#\s*\[\s*tokio::test\s*\]"),
    "criterion_group": re.compile(r"\bcriterion_group!"),
    "criterion_main": re.compile(r"\bcriterion_main!"),
    "proptest": re.compile(r"\bproptest!"),
    "quickcheck": re.compile(r"\bquickcheck!"),
    "fuzz_target": re.compile(r"\bfuzz_target!"),
    "admitted": re.compile(r"\bAdmitted\b"),
    "sorry": re.compile(r"\bsorry\b"),
    "axiom": re.compile(r"\b(?:axiom|Axiom)\b"),
    "assume": re.compile(r"\bassume\b"),
    "todo_fixme_xxx": re.compile(r"\b(?:TODO|FIXME|XXX)\b"),
}


def utc_now() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat()


def relative(path: Path, root: Path) -> str:
    return path.relative_to(root).as_posix()


def top_level(rel: str) -> str:
    return rel.split("/", 1)[0] if "/" in rel else "[root]"


def category(rel: str) -> str:
    top = rel.split("/", 1)[0]
    if top == ".git":
        return "vcs_internal"
    if top == ".claude":
        return "agent_workflow_state"
    if top == "target":
        return "build_product"
    if top == "out":
        return "runtime_output"
    if top == "implementer":
        return "audit_evidence"
    return "authoritative_corpus"


def extension(rel: str) -> str:
    name = rel.rsplit("/", 1)[-1]
    suffix = Path(name).suffix.lower()
    return suffix[1:] if suffix else "[no_ext]"


def looks_text(sample: bytes) -> bool:
    if not sample:
        return True
    if b"\x00" in sample:
        return False
    try:
        sample.decode("utf-8")
        return True
    except UnicodeDecodeError:
        decoded = sample.decode("utf-8", errors="replace")
        replacements = decoded.count("\ufffd")
        return replacements / max(len(decoded), 1) < 0.01


def json_line(writer: TextIO, value: object) -> None:
    writer.write(json.dumps(value, sort_keys=True, ensure_ascii=False))
    writer.write("\n")


def deterministic_gzip_text(path: Path) -> tuple[gzip.GzipFile, TextIO]:
    raw = gzip.GzipFile(filename=str(path), mode="wb", compresslevel=6, mtime=0)
    text = io.TextIOWrapper(raw, encoding="utf-8", newline="\n")
    return raw, text


def scan_regular_file(path: Path) -> tuple[str, bool, int | None, bytes]:
    digest = hashlib.sha256()
    line_breaks = 0
    total = 0
    last_byte = b""
    sample = bytearray()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            if len(sample) < 65536:
                sample.extend(chunk[: 65536 - len(sample)])
            digest.update(chunk)
            line_breaks += chunk.count(b"\n")
            total += len(chunk)
            last_byte = chunk[-1:]
    is_text = looks_text(bytes(sample))
    lines = None
    if is_text:
        lines = line_breaks + (1 if total and last_byte != b"\n" else 0)
    return digest.hexdigest(), is_text, lines, bytes(sample)


def scan_markers(path: Path, rel: str, marker_hits: dict[str, list[dict[str, object]]]) -> None:
    try:
        with path.open("r", encoding="utf-8", errors="replace") as handle:
            for line_no, line in enumerate(handle, 1):
                for name, pattern in MARKERS.items():
                    if pattern.search(line):
                        marker_hits[name].append(
                            {
                                "path": rel,
                                "line": line_no,
                                "excerpt": line.strip()[:240],
                            }
                        )
    except OSError:
        return


def snapshot_tree(root: Path) -> tuple[list[Path], list[Path]]:
    files: list[Path] = []
    directories: list[Path] = [root]
    for current, dirnames, filenames in os.walk(root, topdown=True, followlinks=False):
        current_path = Path(current)
        dirnames.sort()
        filenames.sort()
        for dirname in dirnames:
            directories.append(current_path / dirname)
        for filename in filenames:
            files.append(current_path / filename)
    return sorted(files), sorted(directories)


def write_json(path: Path, value: object) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--out", type=Path, default=Path("forensics/generated"))
    parser.add_argument("--progress-every", type=int, default=25000)
    args = parser.parse_args()

    root = args.root.expanduser().resolve()
    started = utc_now()
    files, directories = snapshot_tree(root)

    out = args.out.expanduser()
    if not out.is_absolute():
        out = root / out
    out.mkdir(parents=True, exist_ok=True)

    inventory_path = out / "all_files_inventory.jsonl.gz"
    directory_path = out / "all_directories_inventory.jsonl.gz"
    authoritative_path = out / "authoritative_files.json"
    summary_path = out / "whole_tree_summary.json"
    marker_path = out / "marker_inventory.json"
    errors_path = out / "scan_errors.json"

    by_category: collections.Counter[str] = collections.Counter()
    bytes_by_category: collections.Counter[str] = collections.Counter()
    lines_by_category: collections.Counter[str] = collections.Counter()
    by_top: collections.Counter[str] = collections.Counter()
    bytes_by_top: collections.Counter[str] = collections.Counter()
    by_extension: collections.Counter[str] = collections.Counter()
    lines_by_extension: collections.Counter[str] = collections.Counter()
    kinds: collections.Counter[str] = collections.Counter()
    authoritative: list[dict[str, object]] = []
    marker_hits: dict[str, list[dict[str, object]]] = collections.defaultdict(list)
    errors: list[dict[str, str]] = []
    largest: list[tuple[int, str, str]] = []
    tree_digest = hashlib.sha256()
    total_read = 0

    raw_gzip, inventory = deterministic_gzip_text(inventory_path)
    try:
        for index, path in enumerate(files, 1):
            rel = relative(path, root)
            cat = category(rel)
            top = top_level(rel)
            ext = extension(rel)
            try:
                st = path.lstat()
                base: dict[str, object] = {
                    "path": rel,
                    "category": cat,
                    "top_level": top,
                    "size": st.st_size,
                    "mtime_ns": st.st_mtime_ns,
                    "mode": oct(stat.S_IMODE(st.st_mode)),
                }
                if stat.S_ISLNK(st.st_mode):
                    target = os.readlink(path)
                    digest = hashlib.sha256(target.encode("utf-8", errors="surrogateescape")).hexdigest()
                    base.update({"kind": "symlink", "target": target, "sha256": digest})
                    kinds["symlink"] += 1
                elif stat.S_ISREG(st.st_mode):
                    digest, is_text, lines, _sample = scan_regular_file(path)
                    base.update(
                        {
                            "kind": "regular",
                            "sha256": digest,
                            "text": is_text,
                            "lines": lines,
                        }
                    )
                    total_read += st.st_size
                    kinds["regular_text" if is_text else "regular_binary"] += 1
                    by_category[cat] += 1
                    bytes_by_category[cat] += st.st_size
                    by_top[top] += 1
                    bytes_by_top[top] += st.st_size
                    by_extension[ext] += 1
                    if lines is not None:
                        lines_by_category[cat] += lines
                        lines_by_extension[ext] += lines
                    largest.append((st.st_size, rel, cat))
                    if cat == "authoritative_corpus":
                        entry = {
                            "path": rel,
                            "bytes": st.st_size,
                            "lines": lines,
                            "text": is_text,
                            "sha256": digest,
                            "extension": ext,
                        }
                        authoritative.append(entry)
                        if is_text:
                            scan_markers(path, rel, marker_hits)
                else:
                    base.update({"kind": "special"})
                    kinds["special"] += 1

                json_line(inventory, base)
                tree_digest.update(rel.encode("utf-8", errors="surrogateescape"))
                tree_digest.update(b"\0")
                tree_digest.update(str(base.get("sha256", "special")).encode("ascii"))
                tree_digest.update(b"\0")
                tree_digest.update(str(st.st_size).encode("ascii"))
                tree_digest.update(b"\n")
            except (OSError, ValueError) as exc:
                errors.append({"path": rel, "error": f"{type(exc).__name__}: {exc}"})
                json_line(inventory, {"path": rel, "kind": "error", "error": str(exc)})

            if index % args.progress_every == 0:
                print(
                    f"scanned {index}/{len(files)} files; read {total_read / (1024**3):.2f} GiB",
                    flush=True,
                )
    finally:
        inventory.flush()
        inventory.detach()
        raw_gzip.close()

    raw_dirs, directory_writer = deterministic_gzip_text(directory_path)
    try:
        for path in directories:
            rel = "." if path == root else relative(path, root)
            try:
                st = path.lstat()
                json_line(
                    directory_writer,
                    {
                        "path": rel,
                        "mode": oct(stat.S_IMODE(st.st_mode)),
                        "mtime_ns": st.st_mtime_ns,
                        "symlink": stat.S_ISLNK(st.st_mode),
                    },
                )
            except OSError as exc:
                errors.append({"path": rel, "error": f"{type(exc).__name__}: {exc}"})
    finally:
        directory_writer.flush()
        directory_writer.detach()
        raw_dirs.close()

    largest.sort(reverse=True)
    marker_document = {
        name: {"count": len(marker_hits.get(name, [])), "hits": marker_hits.get(name, [])}
        for name in MARKERS
    }
    write_json(authoritative_path, authoritative)
    write_json(marker_path, marker_document)
    write_json(errors_path, errors)

    summary = {
        "schema": "anubis-codex-forensics-census-v1",
        "root": str(root),
        "snapshot_started_utc": started,
        "scan_finished_utc": utc_now(),
        "snapshot_boundary": (
            "Paths were snapshotted before output creation. Every snapshotted regular file was "
            "read to EOF and SHA-256 hashed; directories were enumerated without following symlinks."
        ),
        "total_snapshot_files": len(files),
        "total_snapshot_directories": len(directories),
        "total_regular_bytes_read": total_read,
        "total_regular_gib_read": round(total_read / (1024**3), 3),
        "tree_digest_sha256": tree_digest.hexdigest(),
        "kinds": dict(sorted(kinds.items())),
        "files_by_category": dict(sorted(by_category.items())),
        "bytes_by_category": dict(sorted(bytes_by_category.items())),
        "text_lines_by_category": dict(sorted(lines_by_category.items())),
        "files_by_top_level": dict(by_top.most_common()),
        "bytes_by_top_level": dict(bytes_by_top.most_common()),
        "files_by_extension": dict(by_extension.most_common()),
        "text_lines_by_extension": dict(lines_by_extension.most_common()),
        "authoritative_file_count": len(authoritative),
        "authoritative_text_line_count": sum(
            int(entry["lines"] or 0) for entry in authoritative if entry["text"]
        ),
        "largest_files": [
            {"path": rel, "bytes": size, "category": cat}
            for size, rel, cat in largest[:100]
        ],
        "scan_error_count": len(errors),
        "outputs": {
            "file_inventory": inventory_path.name,
            "directory_inventory": directory_path.name,
            "authoritative_inventory": authoritative_path.name,
            "marker_inventory": marker_path.name,
            "errors": errors_path.name,
        },
    }
    write_json(summary_path, summary)
    print(
        json.dumps(
            {
                "files": len(files),
                "directories": len(directories),
                "gib_read": summary["total_regular_gib_read"],
                "authoritative_files": len(authoritative),
                "authoritative_lines": summary["authoritative_text_line_count"],
                "errors": len(errors),
                "tree_digest_sha256": summary["tree_digest_sha256"],
            },
            indent=2,
        ),
        flush=True,
    )
    return 0 if not errors else 2


if __name__ == "__main__":
    raise SystemExit(main())
