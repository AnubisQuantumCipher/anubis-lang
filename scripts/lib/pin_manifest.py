#!/usr/bin/env python3
"""Deterministic, Git-independent source manifest for immutable Anubis pins."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
import sys
from dataclasses import dataclass
from pathlib import Path, PurePosixPath
from typing import Any, NoReturn

POLICY_SCHEMA = "anubis.pin-manifest-policy.v2"
MANIFEST_SCHEMA = "anubis.pin-source-manifest.v2"
DEFAULT_POLICY = "scripts/lib/pin_manifest_policy.json"
POLICY_KEYS = {
    "schema",
    "roots",
    "files",
    "excluded_top_level_entries",
    "excluded_exact_directories",
    "excluded_directory_names",
    "excluded_directory_names_under",
}
TOP_LEVEL_EXCLUSION_KEYS = {"kind", "reason"}
TOP_LEVEL_EXCLUSION_KINDS = {"directory", "file", "file_or_directory"}


def fail(message: str) -> NoReturn:
    print(f"PIN_MANIFEST_ERROR: {message}", file=sys.stderr)
    raise SystemExit(1)


def normalized_relative(raw: object, label: str) -> str:
    if not isinstance(raw, str) or not raw:
        fail(f"{label} must be a non-empty string")
    if "\\" in raw or "\x00" in raw:
        fail(f"{label} is not canonical POSIX syntax: {raw!r}")
    path = PurePosixPath(raw)
    if path.is_absolute() or path.as_posix() != raw or raw == "." or ".." in path.parts:
        fail(f"{label} is not a canonical repository-relative path: {raw!r}")
    return raw


def normalized_name(raw: object, label: str) -> str:
    value = normalized_relative(raw, label)
    if "/" in value:
        fail(f"{label} must be one directory name, not a path: {value!r}")
    return value


def unique_strings(raw: object, label: str, *, names: bool = False) -> tuple[str, ...]:
    if not isinstance(raw, list):
        fail(f"policy {label} must be an array")
    normalize = normalized_name if names else normalized_relative
    values = tuple(normalize(item, f"policy {label} entry") for item in raw)
    if len(values) != len(set(values)):
        fail(f"policy {label} contains a duplicate")
    if tuple(sorted(values)) != values:
        fail(f"policy {label} must be sorted")
    return values


def is_within(path: str, parent: str) -> bool:
    return path == parent or path.startswith(parent + "/")


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def lstat_without_intermediate_symlinks(
    path: Path, label: str, *, allow_missing: bool = False
) -> os.stat_result | None:
    """lstat every path component, rejecting symlinks before the leaf.

    O_NOFOLLOW protects only the final component.  A repository path such as
    ``/trusted/link/repo/file`` would otherwise follow ``link`` before the leaf
    is opened.  Keep the caller's spelling (do not resolve it), so an aliased
    repository or policy path is rejected instead of silently canonicalized.
    """
    if not path.is_absolute():
        fail(f"internal error: {label} path is not absolute: {path}")
    parts = path.parts
    current = Path(path.anchor)
    if len(parts) == 1:
        try:
            return current.lstat()
        except FileNotFoundError:
            if allow_missing:
                return None
            fail(f"cannot stat {label} {path}: path does not exist")
        except OSError as exc:
            fail(f"cannot stat {label} {path}: {exc}")
    for index, component in enumerate(parts[1:]):
        current /= component
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            if allow_missing:
                return None
            fail(f"cannot stat {label} {path}: missing path component {current}")
        except OSError as exc:
            fail(f"cannot stat {label} {path}: {exc}")
        is_leaf = index == len(parts) - 2
        if not is_leaf and stat.S_ISLNK(metadata.st_mode):
            fail(f"{label} has a symlink intermediate path component: {current}")
    return metadata


def regular_nonsymlink(path: Path, label: str) -> os.stat_result:
    metadata = lstat_without_intermediate_symlinks(path, label)
    assert metadata is not None
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        fail(f"{label} must be a regular non-symlink file: {path}")
    return metadata


def file_receipt(path: Path) -> tuple[str, bool]:
    before = regular_nonsymlink(path, "manifest source")
    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    digest = hashlib.sha256()
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb") as handle:
            opened = os.fstat(handle.fileno())
            if not stat.S_ISREG(opened.st_mode):
                fail(f"manifest source changed type while opening: {path}")
            while chunk := handle.read(1024 * 1024):
                digest.update(chunk)
            after = os.fstat(handle.fileno())
    except OSError as exc:
        fail(f"cannot hash {path}: {exc}")
    try:
        path_after = regular_nonsymlink(path, "manifest source")
    except SystemExit:
        raise
    identity_before = (
        before.st_dev,
        before.st_ino,
        before.st_size,
        before.st_mtime_ns,
        before.st_ctime_ns,
        before.st_mode,
    )
    identity_opened = (
        opened.st_dev,
        opened.st_ino,
        opened.st_size,
        opened.st_mtime_ns,
        opened.st_ctime_ns,
        opened.st_mode,
    )
    identity_after = (
        after.st_dev,
        after.st_ino,
        after.st_size,
        after.st_mtime_ns,
        after.st_ctime_ns,
        after.st_mode,
    )
    identity_path_after = (
        path_after.st_dev,
        path_after.st_ino,
        path_after.st_size,
        path_after.st_mtime_ns,
        path_after.st_ctime_ns,
        path_after.st_mode,
    )
    if (
        identity_before != identity_opened
        or identity_opened != identity_after
        or identity_after != identity_path_after
    ):
        fail(f"manifest source changed or was replaced while hashing: {path}")
    return digest.hexdigest(), bool(after.st_mode & 0o111)


def sha256_file(path: Path) -> str:
    return file_receipt(path)[0]


@dataclass(frozen=True)
class Policy:
    path: str
    sha256: str
    roots: tuple[str, ...]
    files: tuple[str, ...]
    excluded_top_level_entries: tuple[tuple[str, str, str], ...]
    excluded_exact_directories: tuple[str, ...]
    excluded_directory_names: frozenset[str]
    excluded_directory_names_under: tuple[tuple[str, frozenset[str]], ...]

    def excludes_directory(self, path: str) -> bool:
        name = PurePosixPath(path).name
        if name in self.excluded_directory_names:
            return True
        if path in self.excluded_exact_directories:
            return True
        return any(
            is_within(path, parent) and name in names
            for parent, names in self.excluded_directory_names_under
        )

    def contains_excluded_ancestor(self, path: str) -> bool:
        parts = PurePosixPath(path).parts
        return any(self.excludes_directory("/".join(parts[:index])) for index in range(1, len(parts)))


def parse_top_level_exclusions(raw: object) -> tuple[tuple[str, str, str], ...]:
    if not isinstance(raw, dict):
        fail("policy excluded_top_level_entries must be an object")
    if tuple(raw) != tuple(sorted(raw)):
        fail("policy excluded_top_level_entries keys must be sorted")
    rows: list[tuple[str, str, str]] = []
    for raw_name, raw_spec in raw.items():
        name = normalized_name(raw_name, "policy top-level exclusion")
        if not isinstance(raw_spec, dict):
            fail(f"policy top-level exclusion {name!r} must be an object")
        unknown = sorted(set(raw_spec) - TOP_LEVEL_EXCLUSION_KEYS)
        missing = sorted(TOP_LEVEL_EXCLUSION_KEYS - set(raw_spec))
        if unknown or missing:
            fail(
                f"policy top-level exclusion {name!r} keys mismatch: "
                f"missing={missing} unknown={unknown}"
            )
        kind = raw_spec["kind"]
        reason = raw_spec["reason"]
        if kind not in TOP_LEVEL_EXCLUSION_KINDS:
            fail(f"policy top-level exclusion {name!r} has unsupported kind: {kind!r}")
        if (
            not isinstance(reason, str)
            or not reason
            or reason != reason.strip()
            or "\n" in reason
            or "\r" in reason
        ):
            fail(f"policy top-level exclusion {name!r} needs a one-line non-empty reason")
        rows.append((name, kind, reason))
    return tuple(rows)


def validate_top_level_coverage(root: Path, policy: Policy) -> None:
    # Every top-level directory is either a complete trust root or an explicit
    # typed exclusion. Nested generated directories are excluded by exact path.
    bound_directories = {PurePosixPath(source_root).parts[0] for source_root in policy.roots}
    bound_files = set(policy.files)
    excluded = {name: (kind, reason) for name, kind, reason in policy.excluded_top_level_entries}
    overlap = sorted((bound_directories | bound_files) & set(excluded))
    if overlap:
        fail(f"top-level entries are both bound and excluded: {overlap}")
    try:
        entries = sorted(root.iterdir(), key=lambda path: path.name)
    except OSError as exc:
        fail(f"cannot enumerate manifest root {root}: {exc}")
    for entry in entries:
        metadata = lstat_without_intermediate_symlinks(entry, "top-level repository entry")
        assert metadata is not None
        name = entry.name
        if stat.S_ISLNK(metadata.st_mode):
            fail(f"top-level repository entry must not be a symlink: {name}")
        if name in bound_directories:
            if not stat.S_ISDIR(metadata.st_mode):
                fail(f"bound top-level source root must be a directory: {name}")
            continue
        if name in bound_files:
            if not stat.S_ISREG(metadata.st_mode):
                fail(f"bound top-level source file must be a regular file: {name}")
            continue
        if name not in excluded:
            fail(f"unclassified top-level repository entry: {name}")
        kind, _reason = excluded[name]
        is_file = stat.S_ISREG(metadata.st_mode)
        is_directory = stat.S_ISDIR(metadata.st_mode)
        if kind == "file" and not is_file:
            fail(f"excluded top-level entry has wrong type (expected file): {name}")
        if kind == "directory" and not is_directory:
            fail(f"excluded top-level entry has wrong type (expected directory): {name}")
        if kind == "file_or_directory" and not (is_file or is_directory):
            fail(f"excluded top-level entry must be a file or directory: {name}")


def load_policy(root: Path, raw_policy_path: Path) -> Policy:
    policy_rel = normalized_relative(raw_policy_path.as_posix(), "policy path")
    policy_path = root / policy_rel
    regular_nonsymlink(policy_path, "manifest policy")
    try:
        with policy_path.open("r", encoding="utf-8") as handle:
            data = json.load(handle, object_pairs_hook=duplicate_rejecting_object)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as exc:
        fail(f"cannot parse manifest policy {policy_rel}: {exc}")
    if not isinstance(data, dict):
        fail("manifest policy root must be an object")
    unknown = sorted(set(data) - POLICY_KEYS)
    missing = sorted(POLICY_KEYS - set(data))
    if unknown or missing:
        fail(f"manifest policy keys mismatch: missing={missing} unknown={unknown}")
    if data["schema"] != POLICY_SCHEMA:
        fail(f"unsupported manifest policy schema: {data['schema']!r}")

    roots = unique_strings(data["roots"], "roots")
    files = unique_strings(data["files"], "files")
    excluded_top_level = parse_top_level_exclusions(data["excluded_top_level_entries"])
    exact = unique_strings(data["excluded_exact_directories"], "excluded_exact_directories")
    global_names = frozenset(
        unique_strings(data["excluded_directory_names"], "excluded_directory_names", names=True)
    )
    under_raw = data["excluded_directory_names_under"]
    if not isinstance(under_raw, dict):
        fail("policy excluded_directory_names_under must be an object")
    under_rows: list[tuple[str, frozenset[str]]] = []
    for raw_parent, raw_names in under_raw.items():
        parent = normalized_relative(raw_parent, "policy exclusion parent")
        names = frozenset(
            unique_strings(
                raw_names,
                f"excluded_directory_names_under[{parent!r}]",
                names=True,
            )
        )
        under_rows.append((parent, names))
    if tuple(parent for parent, _ in under_rows) != tuple(sorted(parent for parent, _ in under_rows)):
        fail("policy excluded_directory_names_under keys must be sorted")

    if not roots:
        fail("manifest policy has no roots")
    for source_root in roots:
        if len(PurePosixPath(source_root).parts) != 1:
            fail(f"manifest policy roots must be complete top-level directories: {source_root}")
    for source_file in files:
        if len(PurePosixPath(source_file).parts) != 1:
            fail(f"manifest policy files must be top-level files: {source_file}")
    for index, left in enumerate(roots):
        for right in roots[index + 1 :]:
            if is_within(right, left):
                fail(f"manifest policy roots overlap: {left!r} and {right!r}")
    for index, excluded in enumerate(exact):
        if not any(is_within(excluded, source_root) and excluded != source_root for source_root in roots):
            fail(f"exact excluded directory is outside the policy roots: {excluded}")
        for other in exact[index + 1 :]:
            if is_within(other, excluded):
                fail(f"exact excluded directories overlap: {excluded!r} and {other!r}")
        excluded_path = root / excluded
        excluded_metadata = lstat_without_intermediate_symlinks(
            excluded_path, "exact excluded directory", allow_missing=True
        )
        if excluded_metadata is not None:
            if stat.S_ISLNK(excluded_metadata.st_mode) or not stat.S_ISDIR(
                excluded_metadata.st_mode
            ):
                fail(f"exact excluded directory must be a real directory when present: {excluded}")
    for parent, _ in under_rows:
        if not any(is_within(parent, source_root) for source_root in roots):
            fail(f"named-exclusion parent is outside the policy roots: {parent}")

    policy = Policy(
        path=policy_rel,
        sha256=sha256_file(policy_path),
        roots=roots,
        files=files,
        excluded_top_level_entries=excluded_top_level,
        excluded_exact_directories=exact,
        excluded_directory_names=global_names,
        excluded_directory_names_under=tuple(under_rows),
    )
    for explicit in files:
        if policy.contains_excluded_ancestor(explicit):
            fail(f"explicit source file is inside an excluded directory: {explicit}")
    validate_top_level_coverage(root, policy)
    return policy


def collect(root: Path, policy: Policy) -> list[str]:
    found: set[str] = set()

    def onerror(exc: OSError) -> None:
        fail(f"traversal failed: {exc}")

    for raw_root in policy.roots:
        base = root / raw_root
        base_metadata = lstat_without_intermediate_symlinks(base, "required source root")
        assert base_metadata is not None
        mode = base_metadata.st_mode
        if stat.S_ISLNK(mode) or not stat.S_ISDIR(mode):
            fail(f"source root must be a real directory: {raw_root}")
        for directory, dirnames, filenames in os.walk(
            base, topdown=True, followlinks=False, onerror=onerror
        ):
            directory_path = Path(directory)
            retained_directories: list[str] = []
            for dirname in sorted(dirnames):
                child = directory_path / dirname
                try:
                    child_mode = child.lstat().st_mode
                except OSError as exc:
                    fail(f"cannot stat source directory {child}: {exc}")
                relative = child.relative_to(root).as_posix()
                if stat.S_ISLNK(child_mode):
                    fail(f"symlink directory in source trust universe: {relative}")
                if not stat.S_ISDIR(child_mode):
                    fail(f"non-directory traversal entry in source trust universe: {relative}")
                if not policy.excludes_directory(relative):
                    retained_directories.append(dirname)
            dirnames[:] = retained_directories
            for filename in sorted(filenames):
                path = directory_path / filename
                regular_nonsymlink(path, "manifest source")
                found.add(path.relative_to(root).as_posix())

    for raw_file in policy.files:
        path = root / raw_file
        regular_nonsymlink(path, "explicit source")
        found.add(raw_file)

    paths = sorted(found)
    if not paths:
        fail("source manifest is empty")
    if policy.path not in found:
        fail(f"manifest policy must bind itself through a root or explicit file: {policy.path}")
    return paths


def directory_receipts(root: Path, policy: Policy) -> dict[str, tuple[int, ...]]:
    """Bind the directory identities whose entry sets define the source universe."""

    receipts: dict[str, tuple[int, ...]] = {}

    def receipt(path: Path, label: str) -> tuple[int, ...]:
        metadata = lstat_without_intermediate_symlinks(path, label)
        assert metadata is not None
        if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
            fail(f"{label} must be a real directory: {path}")
        return (
            metadata.st_dev,
            metadata.st_ino,
            metadata.st_size,
            metadata.st_mtime_ns,
            metadata.st_ctime_ns,
            metadata.st_mode,
        )

    receipts["."] = receipt(root, "manifest root")
    for raw_root in policy.roots:
        base = root / raw_root
        for directory, dirnames, _filenames in os.walk(base, topdown=True, followlinks=False):
            directory_path = Path(directory)
            relative = directory_path.relative_to(root).as_posix()
            receipts[relative] = receipt(directory_path, "source-universe directory")
            retained_directories: list[str] = []
            for dirname in sorted(dirnames):
                child_relative = (directory_path / dirname).relative_to(root).as_posix()
                if not policy.excludes_directory(child_relative):
                    retained_directories.append(dirname)
            dirnames[:] = retained_directories
    return receipts


def build_manifest(root: Path, paths: list[str], policy: Policy) -> dict[str, object]:
    list_bytes = json.dumps(paths, ensure_ascii=True, separators=(",", ":")).encode("ascii")
    tree = hashlib.sha256()
    rows: list[dict[str, object]] = []
    for relative in paths:
        digest, executable = file_receipt(root / relative)
        row: dict[str, object] = {
            "executable": executable,
            "path": relative,
            "sha256": digest,
        }
        tree.update(
            json.dumps(row, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode(
                "ascii"
            )
            + b"\n"
        )
        rows.append(row)
    return {
        "schema": MANIFEST_SCHEMA,
        "policy_path": policy.path,
        "policy_schema": POLICY_SCHEMA,
        "policy_sha256": policy.sha256,
        "count": len(paths),
        "list_sha256": hashlib.sha256(list_bytes).hexdigest(),
        "tree_sha256": tree.hexdigest(),
        "rows": rows,
    }


def stable_manifest(
    root: Path,
    policy_path: Path = Path(DEFAULT_POLICY),
    *,
    opening_policy: Policy | None = None,
    opening_paths: list[str] | None = None,
) -> dict[str, object]:
    """Build three matching content passes closed by directory identities."""

    policy = opening_policy if opening_policy is not None else load_policy(root, policy_path)
    paths = opening_paths if opening_paths is not None else collect(root, policy)
    opening_directories = directory_receipts(root, policy)
    manifest = build_manifest(root, paths, policy)

    policy_after = load_policy(root, policy_path)
    paths_after = collect(root, policy_after)
    manifest_after = build_manifest(root, paths_after, policy_after)
    closing_directories = directory_receipts(root, policy_after)

    policy_final = load_policy(root, policy_path)
    paths_final = collect(root, policy_final)
    final_manifest = build_manifest(root, paths_final, policy_final)
    final_directories = directory_receipts(root, policy_final)
    if (
        policy_after != policy
        or policy_final != policy
        or paths_after != paths
        or paths_final != paths
        or manifest_after != manifest
        or final_manifest != manifest
        or closing_directories != opening_directories
        or final_directories != opening_directories
    ):
        fail("source trust universe changed while building the manifest")
    return manifest


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--policy", type=Path, default=Path(DEFAULT_POLICY))
    # Compatibility assertions for older callers. Supplying these cannot widen or
    # narrow the policy: each list must exactly match the versioned policy.
    parser.add_argument("--dir", action="append", dest="dirs")
    parser.add_argument("--file", action="append", dest="files")
    parser.add_argument(
        "--field",
        choices=(
            "count",
            "list_sha256",
            "tree_sha256",
            "policy_sha256",
            "policy_schema",
            "summary",
            "json",
        ),
        default="json",
    )
    parser.add_argument("--newer-than", type=Path)
    parser.add_argument("--print-rsync-excludes", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    root = Path(os.path.abspath(args.root))
    root_metadata = lstat_without_intermediate_symlinks(root, "manifest root")
    assert root_metadata is not None
    root_mode = root_metadata.st_mode
    if stat.S_ISLNK(root_mode) or not stat.S_ISDIR(root_mode):
        fail(f"manifest root must be a real directory: {root}")
    policy = load_policy(root, args.policy)
    # Exclusion export is part of the guest-sync trust boundary.  Validate the
    # complete source universe before emitting even one filter; otherwise a
    # symlink or malformed entry under a bound root could hide behind the early
    # export path and reach rsync without ever being inspected.
    paths = collect(root, policy)
    if args.print_rsync_excludes:
        if policy.excluded_directory_names or policy.excluded_directory_names_under:
            fail("rsync exclusion export requires exact-directory-only policy exclusions")
        stable_manifest(
            root,
            args.policy,
            opening_policy=policy,
            opening_paths=paths,
        )
        for name, kind, _reason in policy.excluded_top_level_entries:
            suffix = "/" if kind == "directory" else ""
            print(f"/{name}{suffix}")
        for excluded in policy.excluded_exact_directories:
            print(f"/{excluded}/")
        return 0
    if args.dirs is not None and (
        len(args.dirs) != len(set(args.dirs)) or tuple(sorted(args.dirs)) != policy.roots
    ):
        fail("--dir values must exactly match the versioned manifest policy roots")
    if args.files is not None and (
        len(args.files) != len(set(args.files)) or tuple(sorted(args.files)) != policy.files
    ):
        fail("--file values must exactly match the versioned manifest policy files")
    if args.newer_than is not None:
        reference = args.newer_than
        if not reference.is_absolute():
            reference = root / reference
        reference_metadata = regular_nonsymlink(reference, "staleness reference")
        newer = [
            relative
            for relative in paths
            if regular_nonsymlink(root / relative, "manifest source").st_mtime_ns
            > reference_metadata.st_mtime_ns
        ]
        if newer:
            print(newer[0])
            return 3
        return 0
    manifest = stable_manifest(
        root,
        args.policy,
        opening_policy=policy,
        opening_paths=paths,
    )
    if args.field == "json":
        json.dump(manifest, sys.stdout, sort_keys=True, separators=(",", ":"))
        sys.stdout.write("\n")
    elif args.field == "summary":
        print(f"manifest_schema: {manifest['schema']}")
        print(f"policy_sha256: {manifest['policy_sha256']}")
        print(f"src_count: {manifest['count']}")
        print(f"src_list_sha256: {manifest['list_sha256']}")
        print(f"src_tree: {manifest['tree_sha256']}")
    else:
        print(manifest[args.field])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
