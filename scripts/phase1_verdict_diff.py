#!/usr/bin/python3 -I
"""Fail-closed Phase-1 old/new `anubis check` acceptance-class diff producer."""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
import platform
import re
import signal
import stat
import subprocess
import sys
import tempfile
import time
from pathlib import Path, PurePosixPath
from typing import Any


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def stat_identity(
    metadata: os.stat_result,
) -> tuple[int, int, int, int, int, int, int, int]:
    """Return the path identity fields that must remain stable around an open fd."""
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
        metadata.st_mode,
        metadata.st_uid,
        metadata.st_gid,
    )


def receipt_from_stat(
    path: Path, digest: str, metadata: os.stat_result
) -> dict[str, Any]:
    mode = stat.S_IMODE(metadata.st_mode)
    return {
        "path": str(path),
        "sha256": digest,
        "size_bytes": metadata.st_size,
        "mode_octal": f"{mode:04o}",
        "executable": bool(mode & 0o111),
        "writable": bool(mode & 0o222),
        "owner_uid": metadata.st_uid,
        "group_gid": metadata.st_gid,
        "path_identity": {
            "device": metadata.st_dev,
            "inode": metadata.st_ino,
            "size_bytes": metadata.st_size,
            "mtime_ns": metadata.st_mtime_ns,
            "ctime_ns": metadata.st_ctime_ns,
            "mode_octal": f"{mode:04o}",
            "owner_uid": metadata.st_uid,
            "group_gid": metadata.st_gid,
        },
    }


def open_stable_regular(
    path: Path,
    label: str,
    *,
    require_executable: bool = False,
    require_nonwritable: bool = False,
    capture_bytes: bool = False,
) -> tuple[bytes | None, dict[str, Any]]:
    """Hash one stable regular file through O_NOFOLLOW and bind it to its path."""
    try:
        before = path.lstat()
    except OSError as exc:
        raise SystemExit(f"cannot inspect {label}: {path}: {exc}") from exc
    if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
        raise SystemExit(f"{label} must be a regular non-symlink file: {path}")
    if require_executable and not before.st_mode & 0o111:
        raise SystemExit(f"{label} must be executable: {path}")
    if require_nonwritable and before.st_mode & 0o222:
        raise SystemExit(f"{label} must be non-writable: {path}")

    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    digest = hashlib.sha256()
    captured = bytearray() if capture_bytes else None
    try:
        descriptor = os.open(path, flags)
        try:
            opened = os.fstat(descriptor)
            if not stat.S_ISREG(opened.st_mode):
                raise SystemExit(f"{label} changed type while opening: {path}")
            while True:
                chunk = os.read(descriptor, 1024 * 1024)
                if not chunk:
                    break
                digest.update(chunk)
                if captured is not None:
                    captured.extend(chunk)
            after = os.fstat(descriptor)
        finally:
            os.close(descriptor)
        path_after = path.lstat()
    except OSError as exc:
        raise SystemExit(f"cannot read stable {label}: {path}: {exc}") from exc

    identities = (
        stat_identity(before),
        stat_identity(opened),
        stat_identity(after),
        stat_identity(path_after),
    )
    if len(set(identities)) != 1:
        raise SystemExit(f"{label} changed or was replaced while reading: {path}")
    return (bytes(captured) if captured is not None else None), receipt_from_stat(
        path, digest.hexdigest(), after
    )


def write_all(descriptor: int, data: bytes) -> None:
    view = memoryview(data)
    while view:
        written = os.write(descriptor, view)
        if written <= 0:
            raise OSError("short write while creating private snapshot")
        view = view[written:]


def stable_snapshot_regular(
    source: Path,
    destination: Path,
    label: str,
    *,
    require_executable: bool = False,
    require_nonwritable: bool = False,
    snapshot_executable: bool | None = None,
    capture_bytes: bool = False,
) -> tuple[bytes | None, dict[str, Any], dict[str, Any]]:
    """Copy one path-stable source fd into a new private, immutable snapshot."""
    try:
        before = source.lstat()
    except OSError as exc:
        raise SystemExit(f"cannot inspect {label}: {source}: {exc}") from exc
    if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
        raise SystemExit(f"{label} must be a regular non-symlink file: {source}")
    if require_executable and not before.st_mode & 0o111:
        raise SystemExit(f"{label} must be executable: {source}")
    if require_nonwritable and before.st_mode & 0o222:
        raise SystemExit(f"{label} must be non-writable: {source}")

    source_flags = (
        os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    )
    destination_flags = (
        os.O_WRONLY
        | os.O_CREAT
        | os.O_EXCL
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )
    destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    digest = hashlib.sha256()
    captured = bytearray() if capture_bytes else None
    try:
        source_fd = os.open(source, source_flags)
        try:
            opened = os.fstat(source_fd)
            if not stat.S_ISREG(opened.st_mode):
                raise SystemExit(f"{label} changed type while opening: {source}")
            destination_fd = os.open(destination, destination_flags, 0o600)
            try:
                while True:
                    chunk = os.read(source_fd, 1024 * 1024)
                    if not chunk:
                        break
                    digest.update(chunk)
                    if captured is not None:
                        captured.extend(chunk)
                    write_all(destination_fd, chunk)
                source_after = os.fstat(source_fd)
                make_executable = (
                    bool(source_after.st_mode & 0o111)
                    if snapshot_executable is None
                    else snapshot_executable
                )
                os.fchmod(destination_fd, 0o500 if make_executable else 0o400)
                os.fsync(destination_fd)
                destination_after = os.fstat(destination_fd)
            finally:
                os.close(destination_fd)
        finally:
            os.close(source_fd)
        path_after = source.lstat()
    except (OSError, SystemExit) as exc:
        try:
            destination.unlink()
        except FileNotFoundError:
            pass
        if isinstance(exc, SystemExit):
            raise
        raise SystemExit(f"cannot snapshot stable {label}: {source}: {exc}") from exc

    identities = (
        stat_identity(before),
        stat_identity(opened),
        stat_identity(source_after),
        stat_identity(path_after),
    )
    if len(set(identities)) != 1:
        destination.unlink(missing_ok=True)
        raise SystemExit(
            f"{label} changed or was replaced while snapshotting: {source}"
        )
    source_receipt = receipt_from_stat(source, digest.hexdigest(), source_after)
    snapshot_receipt = receipt_from_stat(
        destination, digest.hexdigest(), destination_after
    )
    return (
        bytes(captured) if captured is not None else None,
        source_receipt,
        snapshot_receipt,
    )


def compact_snapshot_receipt(receipt: dict[str, Any]) -> dict[str, Any]:
    return {
        "sha256": receipt["sha256"],
        "size_bytes": receipt["size_bytes"],
        "mode_octal": receipt["mode_octal"],
        "executable": receipt["executable"],
    }


def require_unchanged_path_identity(
    path: Path,
    expected_receipt: dict[str, Any],
    label: str,
) -> None:
    try:
        metadata = path.lstat()
    except OSError as exc:
        raise SystemExit(f"cannot inspect {label}: {path}: {exc}") from exc
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise SystemExit(f"{label} must remain a regular non-symlink file: {path}")
    mode = stat.S_IMODE(metadata.st_mode)
    observed_identity = {
        "device": metadata.st_dev,
        "inode": metadata.st_ino,
        "size_bytes": metadata.st_size,
        "mtime_ns": metadata.st_mtime_ns,
        "ctime_ns": metadata.st_ctime_ns,
        "mode_octal": f"{mode:04o}",
        "owner_uid": metadata.st_uid,
        "group_gid": metadata.st_gid,
    }
    if (
        str(path) != expected_receipt["path"]
        or observed_identity != expected_receipt["path_identity"]
        or not mode & 0o111
        or mode & 0o222
    ):
        raise SystemExit(
            f"{label} path identity changed before checker invocation: {path}"
        )


def resolve_pin_argument(raw: Path, option: str) -> Path:
    try:
        raw_mode = raw.lstat().st_mode
    except OSError as exc:
        raise SystemExit(
            f"cannot inspect {option} path before resolving it: {raw}: {exc}"
        ) from exc
    if stat.S_ISLNK(raw_mode):
        raise SystemExit(f"{option} must not be a final-component symlink alias: {raw}")
    return raw.resolve(strict=True)


def require_real_path_components(root: Path, path: Path, label: str) -> None:
    try:
        relative = path.relative_to(root)
    except ValueError as exc:
        raise SystemExit(f"{label} is outside the repository: {path}") from exc
    current = root
    for index, component in enumerate(relative.parts):
        current /= component
        try:
            metadata = current.lstat()
        except OSError as exc:
            raise SystemExit(
                f"cannot inspect {label} path component {current}: {exc}"
            ) from exc
        if stat.S_ISLNK(metadata.st_mode):
            raise SystemExit(f"{label} must not contain symlink components: {current}")
        if index < len(relative.parts) - 1 and not stat.S_ISDIR(metadata.st_mode):
            raise SystemExit(f"{label} parent component is not a directory: {current}")


def read_current_receipt(root: Path, current_file: Path) -> tuple[Path, dict[str, Any]]:
    """Read one exact canonical CURRENT value from a stable path-bound fd."""
    require_real_path_components(root, current_file, "CURRENT")
    raw, receipt = open_stable_regular(current_file, "CURRENT", capture_bytes=True)
    assert raw is not None
    if not raw or not raw.endswith(b"\n") or raw[:-1].count(b"\n") != 0 or b"\r" in raw:
        raise SystemExit(
            "CURRENT must contain exactly one canonical path followed by one LF"
        )
    try:
        value = raw[:-1].decode("ascii")
    except UnicodeDecodeError as exc:
        raise SystemExit(
            "CURRENT must contain an ASCII repository-relative path"
        ) from exc
    pure = PurePosixPath(value)
    if (
        not value
        or value != value.strip()
        or "\x00" in value
        or "\\" in value
        or pure.is_absolute()
        or pure.as_posix() != value
        or ".." in pure.parts
        or len(pure.parts) != 3
        or pure.parts[:2] != ("vm", "pins")
    ):
        raise SystemExit("CURRENT must contain one canonical vm/pins/<pin> path")
    target = root / pure
    try:
        resolved_target = target.resolve(strict=True)
    except OSError as exc:
        raise SystemExit(f"CURRENT target cannot be resolved: {value}: {exc}") from exc
    return resolved_target, {**receipt, "value": value, "target": str(resolved_target)}


SYSTEM_BASH = Path("/bin/bash")
SYSTEM_PYTHON = Path("/usr/bin/python3")
PIN_VERIFY_ENV_SCHEMA = "anubis.phase1.verdict-diff.pin-verification-environment.v1"
XCODE_PYTHON_ALIAS = Path("/Applications/Xcode.app/Contents/Developer/usr/bin/python3")
XCODE_PYTHON_VERSIONS = Path(
    "/Applications/Xcode.app/Contents/Developer/Library/Frameworks/"
    "Python3.framework/Versions"
)


def require_root_owned_protected_receipt(receipt: dict[str, Any], label: str) -> None:
    """Require a root-owned file that an unprivileged caller cannot replace in place."""
    mode = int(receipt["mode_octal"], 8)
    if receipt["owner_uid"] != 0 or mode & 0o022:
        raise SystemExit(
            f"{label} must be root-owned and not group/world writable: {receipt['path']}"
        )


def stable_symlink_receipt(path: Path, label: str) -> dict[str, Any]:
    """Read one symlink target while proving that the directory entry stayed stable."""
    try:
        before = path.lstat()
        if not stat.S_ISLNK(before.st_mode):
            raise SystemExit(f"{label} must be a symlink: {path}")
        target = os.readlink(path)
        after = path.lstat()
    except OSError as exc:
        raise SystemExit(f"cannot inspect stable {label}: {path}: {exc}") from exc
    if stat_identity(before) != stat_identity(after):
        raise SystemExit(f"{label} changed or was replaced while reading: {path}")
    mode = stat.S_IMODE(after.st_mode)
    return {
        "path": str(path),
        "target": target,
        "target_sha256": hashlib.sha256(target.encode()).hexdigest(),
        "mode_octal": f"{mode:04o}",
        "owner_uid": after.st_uid,
        "group_gid": after.st_gid,
        "path_identity": {
            "device": after.st_dev,
            "inode": after.st_ino,
            "size_bytes": after.st_size,
            "mtime_ns": after.st_mtime_ns,
            "ctime_ns": after.st_ctime_ns,
            "mode_octal": f"{mode:04o}",
            "owner_uid": after.st_uid,
            "group_gid": after.st_gid,
        },
    }


def validated_system_bash() -> tuple[Path, dict[str, Any]]:
    """Bind pin verification to the immutable root-owned /bin/bash executable."""
    _, receipt = open_stable_regular(
        SYSTEM_BASH,
        "system Bash",
        require_executable=True,
        require_nonwritable=True,
    )
    require_root_owned_protected_receipt(receipt, "system Bash")
    return SYSTEM_BASH, receipt


def validated_system_python() -> tuple[Path, dict[str, Any]]:
    """Bind helper launch to Apple's root-owned /usr/bin/python3 shim."""
    _, receipt = open_stable_regular(
        SYSTEM_PYTHON,
        "system Python",
        require_executable=True,
    )
    require_root_owned_protected_receipt(receipt, "system Python launcher")
    return SYSTEM_PYTHON, receipt


def isolated_python_command(helper: Path, *args: str) -> list[str]:
    python, _ = validated_system_python()
    return [str(python), "-I", "-B", str(helper), *args]


def require_isolated_python_runtime() -> dict[str, Any]:
    python, launcher_receipt = validated_system_python()
    if not (
        sys.flags.isolated == 1
        and sys.flags.ignore_environment == 1
        and sys.flags.no_user_site == 1
    ):
        raise SystemExit(
            "Phase-1 verdict diff requires isolated Python startup; invoke exactly: "
            "/usr/bin/python3 -I -B scripts/phase1_verdict_diff.py ..."
        )

    if sys.executable != str(XCODE_PYTHON_ALIAS):
        raise SystemExit(
            "Phase-1 verdict diff requires the Apple system Python runtime selected by "
            f"{python}; invoke exactly: "
            "/usr/bin/python3 -I -B scripts/phase1_verdict_diff.py ..."
        )
    alias_parent_receipts = require_root_owned_real_directory_chain(
        XCODE_PYTHON_ALIAS.parent,
        "Xcode Python alias parent",
    )
    alias_open = stable_symlink_receipt(
        XCODE_PYTHON_ALIAS, "Xcode Python executable alias"
    )
    if alias_open["owner_uid"] != 0:
        raise SystemExit(
            f"Xcode Python executable alias must be root-owned: {XCODE_PYTHON_ALIAS}"
        )
    try:
        runtime_executable = XCODE_PYTHON_ALIAS.resolve(strict=True)
        relative_runtime = runtime_executable.relative_to(XCODE_PYTHON_VERSIONS)
    except (OSError, ValueError) as exc:
        raise SystemExit(
            "Xcode Python executable must resolve inside its versioned framework: "
            f"{XCODE_PYTHON_ALIAS}"
        ) from exc
    if (
        len(relative_runtime.parts) != 3
        or not re.fullmatch(r"[0-9]+\.[0-9]+", relative_runtime.parts[0])
        or relative_runtime.parts[1] != "bin"
        or relative_runtime.parts[2] != f"python{relative_runtime.parts[0]}"
    ):
        raise SystemExit(f"unexpected Xcode Python runtime path: {runtime_executable}")
    runtime_parent_receipts = require_root_owned_real_path_components(
        runtime_executable,
        "Xcode Python runtime",
    )
    _, runtime_receipt = open_stable_regular(
        runtime_executable,
        "Xcode Python runtime executable",
        require_executable=True,
    )
    require_root_owned_protected_receipt(
        runtime_receipt, "Xcode Python runtime executable"
    )
    alias_close = stable_symlink_receipt(
        XCODE_PYTHON_ALIAS, "Xcode Python executable alias"
    )
    if (
        alias_open != alias_close
        or XCODE_PYTHON_ALIAS.resolve(strict=True) != runtime_executable
    ):
        raise SystemExit(
            "Xcode Python executable alias changed during runtime validation"
        )
    return {
        "launcher": launcher_receipt,
        "reported_executable": sys.executable,
        "runtime_alias": alias_open,
        "runtime_executable": runtime_receipt,
        "alias_parent_directories": alias_parent_receipts,
        "runtime_parent_directories": runtime_parent_receipts,
        "flags": {
            "isolated": sys.flags.isolated,
            "ignore_environment": sys.flags.ignore_environment,
            "no_user_site": sys.flags.no_user_site,
            "dont_write_bytecode": sys.flags.dont_write_bytecode,
        },
        "required_invocation": "/usr/bin/python3 -I -B scripts/phase1_verdict_diff.py ...",
    }


def pin_verification_environment() -> tuple[dict[str, str], dict[str, Any]]:
    """Build the exact non-inheriting environment for publish_pin.sh --verify."""
    environment = {
        "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_OPTIONAL_LOCKS": "0",
        "GIT_TERMINAL_PROMPT": "0",
        "HOME": "/var/empty",
        "LANG": "C",
        "LC_ALL": "C",
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
        "TMPDIR": "/private/tmp",
        "TZ": "UTC",
    }
    path_directories = protected_system_path_receipts(
        environment["PATH"], "pin verification"
    )
    contract: dict[str, Any] = {
        "schema": PIN_VERIFY_ENV_SCHEMA,
        "inheritance": "none",
        "variables": dict(sorted(environment.items())),
        "allowed_variable_names": sorted(environment),
        "argv_prefix": [str(SYSTEM_BASH), "--noprofile", "--norc"],
        "path_directories": path_directories,
        "discarded_caller_environment": "all variables, including BASH_ENV, ENV, SHELLOPTS, "
        "BASHOPTS, function exports, DYLD_*, LD_*, PYTHON*, GIT_*, and ANUBIS_*",
        "digest_contract": {
            "algorithm": "sha256",
            "encoding": "canonical JSON: ASCII, sorted keys, compact separators",
            "excluded_field": "contract_sha256",
        },
    }
    contract["contract_sha256"] = hashlib.sha256(
        canonical_json_bytes(contract)
    ).hexdigest()
    return environment, contract


def run_pin_verification(
    root: Path,
    bash: Path,
    *,
    environment: dict[str, str],
) -> dict[str, Any]:
    return run_capture(
        [str(bash), "--noprofile", "--norc", "scripts/publish_pin.sh", "--verify"],
        root,
        env=dict(environment),
    )


def open_directory_chain_no_symlinks(directory: Path) -> int:
    """Open/create an absolute directory chain without following any component."""
    if not directory.is_absolute():
        raise SystemExit(f"output parent must be absolute: {directory}")
    flags = (
        os.O_RDONLY
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_DIRECTORY", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )
    descriptor = os.open(directory.anchor, flags)
    try:
        for component in directory.parts[1:]:
            try:
                child = os.open(component, flags, dir_fd=descriptor)
            except FileNotFoundError:
                try:
                    os.mkdir(component, 0o700, dir_fd=descriptor)
                except FileExistsError:
                    pass
                child = os.open(component, flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        return descriptor
    except OSError as exc:
        os.close(descriptor)
        raise SystemExit(
            f"output parent contains an invalid or symlinked component: {directory}: {exc}"
        ) from exc


def publish_report_no_clobber(out: Path, report: dict[str, Any]) -> None:
    out = Path(os.path.abspath(out))
    directory_fd = open_directory_chain_no_symlinks(out.parent)
    temporary_name = out.name + f".tmp.{os.getpid()}.{time.monotonic_ns()}"
    temporary_fd: int | None = None
    try:
        temporary_fd = os.open(
            temporary_name,
            os.O_WRONLY
            | os.O_CREAT
            | os.O_EXCL
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0),
            0o600,
            dir_fd=directory_fd,
        )
        payload = (json.dumps(report, indent=2, sort_keys=True) + "\n").encode()
        write_all(temporary_fd, payload)
        os.fsync(temporary_fd)
        os.close(temporary_fd)
        temporary_fd = None
        try:
            # Same-directory hard-link publication is atomic and refuses an existing
            # destination. Unlike os.replace(), it cannot overwrite an output created
            # by another worker during the long verdict-diff run.
            os.link(
                temporary_name,
                out.name,
                src_dir_fd=directory_fd,
                dst_dir_fd=directory_fd,
                follow_symlinks=False,
            )
        except FileExistsError as exc:
            raise SystemExit(
                f"refusing to overwrite concurrently created output: {out}"
            ) from exc
        os.fsync(directory_fd)
    finally:
        if temporary_fd is not None:
            os.close(temporary_fd)
        try:
            os.unlink(temporary_name, dir_fd=directory_fd)
        except FileNotFoundError:
            pass
        os.close(directory_fd)


def run_capture(
    argv: list[str],
    cwd: Path,
    timeout: int = 120,
    *,
    env: dict[str, str] | None = None,
) -> dict[str, Any]:
    started = time.monotonic()
    try:
        proc = subprocess.run(
            argv,
            cwd=cwd,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
            env=env,
        )
        rc = proc.returncode
        return {
            "argv": argv,
            "rc": rc,
            "stdout": proc.stdout.strip(),
            "stderr": proc.stderr.strip(),
            "elapsed_seconds": round(time.monotonic() - started, 3),
            "timeout": False,
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "argv": argv,
            "rc": None,
            "stdout": (exc.stdout or "").strip() if isinstance(exc.stdout, str) else "",
            "stderr": (exc.stderr or "").strip() if isinstance(exc.stderr, str) else "",
            "elapsed_seconds": round(time.monotonic() - started, 3),
            "timeout": True,
        }


CHECKER_CHILD_ENV_SCHEMA = "anubis.phase1.verdict-diff.checker-environment.v1"
CHECKER_CHILD_HOME = "/var/empty"
CHECKER_CHILD_OS_PATH = "/usr/bin:/bin:/usr/sbin:/sbin"
CHECKER_CHILD_TMPDIR = "/private/tmp"
SYSTEM_OTOOL = Path("/usr/bin/otool")


def require_absolute_real_path_components(path: Path, label: str) -> None:
    if not path.is_absolute():
        raise SystemExit(f"{label} must be absolute: {path}")
    current = Path(path.anchor)
    for index, component in enumerate(path.parts[1:]):
        current /= component
        try:
            metadata = current.lstat()
        except OSError as exc:
            raise SystemExit(
                f"cannot inspect {label} path component {current}: {exc}"
            ) from exc
        if stat.S_ISLNK(metadata.st_mode):
            raise SystemExit(f"{label} must not contain symlink components: {current}")
        if index < len(path.parts[1:]) - 1 and not stat.S_ISDIR(metadata.st_mode):
            raise SystemExit(f"{label} parent component is not a directory: {current}")


def require_root_owned_real_directory_chain(
    path: Path, label: str
) -> list[dict[str, Any]]:
    """Require every component through `path` to be a protected real directory."""
    if not path.is_absolute():
        raise SystemExit(f"{label} must be absolute: {path}")
    current = Path(path.anchor)
    receipts: list[dict[str, Any]] = []
    for component in path.parts[1:]:
        current /= component
        try:
            metadata = current.lstat()
        except OSError as exc:
            raise SystemExit(
                f"cannot inspect {label} path component {current}: {exc}"
            ) from exc
        mode = stat.S_IMODE(metadata.st_mode)
        group_write_is_standard_applications_exception = (
            current == Path("/Applications") and bool(mode & 0o020) and not mode & 0o002
        )
        if (
            stat.S_ISLNK(metadata.st_mode)
            or not stat.S_ISDIR(metadata.st_mode)
            or metadata.st_uid != 0
            or (mode & 0o022 and not group_write_is_standard_applications_exception)
        ):
            raise SystemExit(
                f"{label} components must be root-owned protected real directories: {current}"
            )
        receipts.append(
            {
                "path": str(current),
                "mode_octal": f"{mode:04o}",
                "owner_uid": metadata.st_uid,
                "group_gid": metadata.st_gid,
                "flags": metadata.st_flags,
                "applications_group_write_exception": group_write_is_standard_applications_exception,
                "path_identity": {
                    "device": metadata.st_dev,
                    "inode": metadata.st_ino,
                    "size_bytes": metadata.st_size,
                    "mtime_ns": metadata.st_mtime_ns,
                    "ctime_ns": metadata.st_ctime_ns,
                    "mode_octal": f"{mode:04o}",
                    "owner_uid": metadata.st_uid,
                    "group_gid": metadata.st_gid,
                },
            }
        )
    return receipts


def require_root_owned_real_path_components(
    path: Path, label: str
) -> list[dict[str, Any]]:
    """Require protected real directories and a root-owned protected regular leaf."""
    parent_receipts = require_root_owned_real_directory_chain(path.parent, label)
    try:
        metadata = path.lstat()
    except OSError as exc:
        raise SystemExit(
            f"cannot inspect {label} path component {path}: {exc}"
        ) from exc
    mode = stat.S_IMODE(metadata.st_mode)
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_uid != 0
        or mode & 0o022
    ):
        raise SystemExit(f"{label} must be a root-owned protected real file: {path}")
    return parent_receipts


def directory_identity_receipt(path: Path, label: str) -> dict[str, Any]:
    try:
        metadata = path.lstat()
    except OSError as exc:
        raise SystemExit(f"cannot inspect {label} directory {path}: {exc}") from exc
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
        raise SystemExit(f"{label} must be a real directory: {path}")
    mode = stat.S_IMODE(metadata.st_mode)
    return {
        "path": str(path),
        "mode_octal": f"{mode:04o}",
        "owner_uid": metadata.st_uid,
        "group_gid": metadata.st_gid,
        "path_identity": {
            "device": metadata.st_dev,
            "inode": metadata.st_ino,
            "size_bytes": metadata.st_size,
            "mtime_ns": metadata.st_mtime_ns,
            "ctime_ns": metadata.st_ctime_ns,
            "mode_octal": f"{mode:04o}",
            "owner_uid": metadata.st_uid,
            "group_gid": metadata.st_gid,
        },
    }


def protected_system_path_receipts(path_value: str, label: str) -> list[dict[str, Any]]:
    """Bind a PATH made only from root-owned, non-writable real directories."""
    components = path_value.split(":")
    if not components or any(
        not component
        or not Path(component).is_absolute()
        or "." in Path(component).parts
        or ".." in Path(component).parts
        for component in components
    ):
        raise SystemExit(
            f"{label} PATH must contain only canonical absolute components"
        )
    receipts: list[dict[str, Any]] = []
    for component in components:
        receipt = directory_identity_receipt(Path(component), f"{label} PATH component")
        if receipt["owner_uid"] != 0 or int(receipt["mode_octal"], 8) & 0o022:
            raise SystemExit(
                f"{label} PATH components must be root-owned and not group/world writable: "
                f"{component}"
            )
        receipts.append(receipt)
    return receipts


def intended_z3_source() -> Path:
    machine = platform.machine()
    if machine in {"arm64", "aarch64"}:
        source = Path("/opt/homebrew/Cellar/z3/4.15.4/bin/z3")
    elif machine == "x86_64":
        source = Path("/usr/local/Cellar/z3/4.15.4/bin/z3")
    else:
        raise SystemExit(f"unsupported machine for pinned Z3 4.15.4 binding: {machine}")
    require_absolute_real_path_components(source, "pinned Z3 source")
    return source


def validated_system_otool() -> tuple[Path, dict[str, Any]]:
    _, receipt = open_stable_regular(
        SYSTEM_OTOOL,
        "system otool",
        require_executable=True,
    )
    require_root_owned_protected_receipt(receipt, "system otool")
    return SYSTEM_OTOOL, receipt


def inspect_z3_dylibs(executable: Path, cwd: Path, otool: Path) -> dict[str, Any]:
    environment = {
        "HOME": CHECKER_CHILD_HOME,
        "LANG": "C",
        "LC_ALL": "C",
        "PATH": CHECKER_CHILD_OS_PATH,
        "TMPDIR": CHECKER_CHILD_TMPDIR,
        "TZ": "UTC",
    }
    receipt = run_capture([str(otool), "-L", str(executable)], cwd, env=environment)
    if receipt["rc"] != 0 or receipt["timeout"]:
        raise SystemExit(f"cannot inspect Z3 dynamic-library dependencies: {receipt}")
    dependencies: list[str] = []
    for raw_line in receipt["stdout"].splitlines()[1:]:
        line = raw_line.strip()
        if not line:
            continue
        dependency = line.split(" (compatibility version", 1)[0]
        dependencies.append(dependency)
    if not dependencies:
        raise SystemExit(
            f"Z3 dependency inspection returned no dependencies: {executable}"
        )
    non_system = [
        dependency
        for dependency in dependencies
        if not dependency.startswith(("/usr/lib/", "/System/Library/"))
    ]
    if non_system:
        raise SystemExit(
            "private Z3 snapshot has non-system or path-relative dynamic dependencies: "
            f"{non_system}"
        )
    return {"command": receipt, "dependencies": dependencies}


def snapshot_checker_z3(
    root: Path,
    snapshot: Path,
) -> tuple[Path, dict[str, Any]]:
    source = intended_z3_source()
    _, source_receipt, snapshot_receipt = stable_snapshot_regular(
        source,
        snapshot,
        "pinned Z3 4.15.4 executable",
        require_executable=True,
        require_nonwritable=True,
        snapshot_executable=True,
    )
    otool, otool_receipt = validated_system_otool()
    source_dylibs = inspect_z3_dylibs(source, root, otool)
    snapshot_dylibs = inspect_z3_dylibs(snapshot, root, otool)
    if source_dylibs["dependencies"] != snapshot_dylibs["dependencies"]:
        raise SystemExit(
            "Z3 snapshot dynamic-library dependencies differ from its source"
        )
    return snapshot, {
        "source": source_receipt,
        "snapshot": snapshot_receipt,
        "otool": otool_receipt,
        "source_dylibs": source_dylibs,
        "snapshot_dylibs": snapshot_dylibs,
    }


def close_checker_z3_binding(
    root: Path,
    source: Path,
    snapshot: Path,
) -> dict[str, Any]:
    _, source_receipt = open_stable_regular(
        source,
        "pinned Z3 source at closure",
        require_executable=True,
        require_nonwritable=True,
    )
    _, snapshot_receipt = open_stable_regular(
        snapshot,
        "private Z3 snapshot at closure",
        require_executable=True,
        require_nonwritable=True,
    )
    otool, otool_receipt = validated_system_otool()
    return {
        "source": source_receipt,
        "snapshot": snapshot_receipt,
        "otool": otool_receipt,
        "source_dylibs": inspect_z3_dylibs(source, root, otool),
        "snapshot_dylibs": inspect_z3_dylibs(snapshot, root, otool),
    }


def compact_checker_z3_binding(binding: dict[str, Any]) -> dict[str, Any]:
    return {
        "source": binding["source"],
        "snapshot": binding["snapshot"],
        "otool": binding["otool"],
        "source_dependencies": binding["source_dylibs"]["dependencies"],
        "snapshot_dependencies": binding["snapshot_dylibs"]["dependencies"],
    }


def checker_child_environment(
    z3_snapshot: Path,
    expected_snapshot_receipt: dict[str, Any],
) -> tuple[dict[str, str], dict[str, Any]]:
    """Build the exact non-inheriting environment for every checker invocation."""
    for label, raw_path, allow_sticky in (
        ("HOME", CHECKER_CHILD_HOME, False),
        ("TMPDIR", CHECKER_CHILD_TMPDIR, True),
    ):
        path = Path(raw_path)
        try:
            metadata = path.lstat()
        except OSError as exc:
            raise SystemExit(f"checker {label} is unavailable: {path}: {exc}") from exc
        if (
            not path.is_absolute()
            or stat.S_ISLNK(metadata.st_mode)
            or not stat.S_ISDIR(metadata.st_mode)
        ):
            raise SystemExit(
                f"checker {label} must be an absolute non-symlink directory: {path}"
            )
        mode = stat.S_IMODE(metadata.st_mode)
        if metadata.st_uid != 0:
            raise SystemExit(f"checker {label} must be root-owned: {path}")
        if allow_sticky:
            if not mode & stat.S_ISVTX:
                raise SystemExit(f"checker {label} must be sticky: {path}")
        elif mode & 0o022:
            raise SystemExit(
                f"checker {label} must not be group/world writable: {path}"
            )

    _, observed_z3_snapshot = open_stable_regular(
        z3_snapshot,
        "private Z3 snapshot for checker environment",
        require_executable=True,
        require_nonwritable=True,
    )
    if observed_z3_snapshot != expected_snapshot_receipt:
        raise SystemExit(
            "private Z3 snapshot identity changed before checker invocation"
        )

    checker_path = f"{z3_snapshot.parent}:{CHECKER_CHILD_OS_PATH}"
    path_components = checker_path.split(":")
    if not path_components or any(
        not component
        or not Path(component).is_absolute()
        or "." in Path(component).parts
        or ".." in Path(component).parts
        for component in path_components
    ):
        raise SystemExit("checker PATH must contain only canonical absolute components")
    private_path_receipt = directory_identity_receipt(
        z3_snapshot.parent,
        "private checker toolchain PATH component",
    )
    if (
        private_path_receipt["owner_uid"] != os.geteuid()
        or private_path_receipt["mode_octal"] != "0700"
    ):
        raise SystemExit(
            "private checker toolchain directory must be caller-owned with exact mode 0700: "
            f"{z3_snapshot.parent}"
        )
    system_path_receipts = protected_system_path_receipts(
        CHECKER_CHILD_OS_PATH,
        "checker",
    )

    environment = {
        "ANUBIS_NATIVE_AUTHORITATIVE": "1",
        "HOME": CHECKER_CHILD_HOME,
        "LANG": "C",
        "LC_ALL": "C",
        "PATH": checker_path,
        "TMPDIR": CHECKER_CHILD_TMPDIR,
        "TZ": "UTC",
    }
    native_keys = {key for key in environment if key.startswith("ANUBIS_NATIVE_")}
    if native_keys != {"ANUBIS_NATIVE_AUTHORITATIVE"}:
        raise SystemExit(
            "checker environment contains an unauthorized ANUBIS_NATIVE_* variable"
        )

    contract: dict[str, Any] = {
        "schema": CHECKER_CHILD_ENV_SCHEMA,
        "inheritance": "none",
        "variables": dict(sorted(environment.items())),
        "allowed_variable_names": sorted(environment),
        "native_authority": {
            "forced": "ANUBIS_NATIVE_AUTHORITATIVE=1",
            "all_other_anubis_native_variables": "discarded",
        },
        "z3_binding": {
            "path": str(z3_snapshot),
            "sha256": expected_snapshot_receipt["sha256"],
            "size_bytes": expected_snapshot_receipt["size_bytes"],
            "mode_octal": expected_snapshot_receipt["mode_octal"],
        },
        "path_directories": [private_path_receipt, *system_path_receipts],
        "digest_contract": {
            "algorithm": "sha256",
            "encoding": "canonical JSON: ASCII, sorted keys, compact separators",
            "excluded_field": "contract_sha256",
        },
        "discarded_caller_families": [
            "ANUBIS_NATIVE_* except ANUBIS_NATIVE_AUTHORITATIVE=1",
            "all other ANUBIS_*",
            "DYLD_*",
            "LD_*",
            "PYTHON*",
            "GIT_*",
            "CARGO_*",
            "RUST*",
            "compiler/linker configuration including CC, CXX, CPP, CFLAGS, CPPFLAGS, LDFLAGS, SDKROOT, and MACOSX_DEPLOYMENT_TARGET",
            "all remaining caller variables",
        ],
    }
    contract["contract_sha256"] = hashlib.sha256(
        canonical_json_bytes(contract)
    ).hexdigest()
    return environment, contract


def invoke(
    binary: Path,
    fixture: Path,
    timeout: int,
    *,
    environment: dict[str, str],
    z3_snapshot: Path,
    z3_snapshot_receipt: dict[str, Any],
) -> dict[str, Any]:
    require_unchanged_path_identity(
        z3_snapshot,
        z3_snapshot_receipt,
        "private Z3 snapshot",
    )
    started = time.monotonic()
    proc = subprocess.Popen(
        [str(binary), "check", str(fixture)],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
        env=dict(environment),
    )
    try:
        proc.wait(timeout=timeout)
        rc = proc.returncode
        return {
            "rc": rc,
            "timeout": False,
            "class": "ACCEPT" if rc == 0 else "REJECT",
            "elapsed_seconds": round(time.monotonic() - started, 3),
        }
    except subprocess.TimeoutExpired:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()
        return {
            "rc": None,
            "timeout": True,
            "class": "TIMEOUT",
            "elapsed_seconds": round(time.monotonic() - started, 3),
        }


def trusted_git_environment() -> dict[str, str]:
    """Return a child environment whose bare `git` is the trusted system binary."""
    git = Path("/usr/bin/git")
    try:
        metadata = git.lstat()
    except OSError as exc:
        raise RuntimeError(f"trusted system Git is unavailable: {git}: {exc}") from exc
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise RuntimeError(
            f"trusted system Git is not a regular non-symlink file: {git}"
        )
    if not metadata.st_mode & 0o111:
        raise RuntimeError(f"trusted system Git is not executable: {git}")

    environment = {
        key: value
        for key, value in os.environ.items()
        if not key.startswith(("GIT_", "DYLD_", "LD_"))
        and key not in {"PYTHONHOME", "PYTHONPATH"}
    }
    environment.update(
        {
            "PATH": "/usr/bin:/bin",
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_OPTIONAL_LOCKS": "0",
            "GIT_TERMINAL_PROMPT": "0",
        }
    )
    return environment


def read_inventory(root: Path, helper: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    receipt = run_capture(
        isolated_python_command(helper, "--json", "--root", str(root)),
        root,
        env=trusted_git_environment(),
    )
    if receipt["rc"] != 0 or receipt["timeout"]:
        raise RuntimeError(f"inventory helper failed: {receipt}")
    parsed = json.loads(receipt["stdout"])
    if not isinstance(parsed, dict) or not isinstance(parsed.get("files"), list):
        raise RuntimeError("inventory helper returned malformed JSON")
    files = parsed["files"]
    if parsed.get("count") != len(files):
        raise RuntimeError("inventory count does not equal file-list length")
    if any(not isinstance(item, str) for item in files):
        raise RuntimeError("inventory contains a non-string path")
    if len(files) != len(set(files)):
        raise RuntimeError("inventory contains duplicate paths")
    if files != sorted(files):
        raise RuntimeError("inventory is not sorted deterministically")
    if any(
        not (item.startswith("examples/") or item.startswith("tests/fixtures/"))
        for item in files
    ):
        raise RuntimeError("inventory escaped authoritative roots")
    return parsed, receipt


HEX64 = re.compile(r"[0-9a-f]{64}")
HEX40 = re.compile(r"[0-9a-f]{40}")
HEX40_OR_64 = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})")
POSITIVE_INT = re.compile(r"[1-9][0-9]*")
CURRENT_MANIFEST_SCHEMA = "anubis.pin-source-manifest.v2"
CURRENT_PIN_SCHEMA = "anubis.binary-pin.v2"


def parse_meta(raw: bytes, path: Path) -> dict[str, str]:
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise SystemExit(f"pin metadata is not UTF-8: {path}: {exc}") from exc
    result: dict[str, str] = {}
    for line_number, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        if ":" not in line:
            raise SystemExit(f"malformed pin metadata line {line_number}: {path}")
        key, value = line.split(":", 1)
        key = key.strip()
        if not key:
            raise SystemExit(f"empty pin metadata key at line {line_number}: {path}")
        if key in result:
            raise SystemExit(f"duplicate pin metadata field {key!r}: {path}")
        result[key] = value.strip()
    return result


def validate_pin_receipt(path: Path, receipt: dict[str, Any]) -> dict[str, Any]:
    if not receipt["executable"]:
        raise SystemExit(f"pin must be executable: {path}")
    if receipt["writable"]:
        raise SystemExit(f"pin must be non-writable: {path}")
    return {**receipt, "path": str(path)}


def pin_receipt(path: Path) -> dict[str, Any]:
    _, receipt = open_stable_regular(
        path,
        "pin",
        require_executable=True,
        require_nonwritable=True,
    )
    return validate_pin_receipt(path, receipt)


def validate_meta_receipt(
    path: Path,
    raw: bytes,
    receipt: dict[str, Any],
    binary_sha256: str,
    expected_pin: str,
    *,
    require_modern_fields: bool,
) -> dict[str, Any]:
    if receipt["writable"]:
        raise SystemExit(f"pin metadata must be non-writable: {path}")
    fields = parse_meta(raw, path)
    if fields.get("pin") != expected_pin:
        raise SystemExit(
            f"pin metadata pin field does not match binary path: {path}: "
            f"observed={fields.get('pin')!r} expected={expected_pin!r}"
        )
    if not HEX64.fullmatch(fields.get("sha256", "")):
        raise SystemExit(f"pin metadata sha256 must be 64 lowercase hex: {path}")
    if fields.get("sha256") != binary_sha256:
        raise SystemExit(f"pin metadata sha256 does not match binary: {path}")
    if not HEX64.fullmatch(fields.get("src_tree", "")):
        raise SystemExit(f"pin metadata src_tree must be 64 lowercase hex: {path}")
    has_count = "src_count" in fields
    has_list = "src_list_sha256" in fields
    if has_count != has_list:
        raise SystemExit(
            f"pin metadata src_count and src_list_sha256 must be both present or both absent: {path}"
        )
    if require_modern_fields and not (has_count and has_list):
        raise SystemExit(
            f"current pin metadata requires src_count and src_list_sha256: {path}"
        )
    if (
        require_modern_fields
        and fields.get("manifest_schema") != CURRENT_MANIFEST_SCHEMA
    ):
        raise SystemExit(
            f"current pin metadata requires manifest_schema {CURRENT_MANIFEST_SCHEMA}: {path}"
        )
    if require_modern_fields and not HEX64.fullmatch(fields.get("policy_sha256", "")):
        raise SystemExit(
            f"current pin metadata requires a 64-hex policy_sha256: {path}"
        )
    if has_count and not POSITIVE_INT.fullmatch(fields["src_count"]):
        raise SystemExit(f"pin metadata src_count must be a positive integer: {path}")
    if has_list and not HEX64.fullmatch(fields["src_list_sha256"]):
        raise SystemExit(
            f"pin metadata src_list_sha256 must be 64 lowercase hex: {path}"
        )
    if "pin_schema" in fields:
        if fields["pin_schema"] != CURRENT_PIN_SCHEMA:
            raise SystemExit(f"unsupported pin_schema in metadata: {path}")
        if not HEX40.fullmatch(fields.get("head", "")):
            raise SystemExit(
                f"versioned pin metadata head must be 40 lowercase hex: {path}"
            )
        if not HEX40_OR_64.fullmatch(fields.get("head_tree", "")):
            raise SystemExit(
                f"versioned pin metadata head_tree must be 40 or 64 lowercase hex: {path}"
            )
        if fields.get("commit_bound") not in ("true", "false"):
            raise SystemExit(
                f"versioned pin metadata commit_bound must be true or false: {path}"
            )
        expected_build_mode = (
            "cargo-build-locked-release-exact-head-archive-clean-target"
            if fields["commit_bound"] == "true"
            else "technical-existing-target"
        )
        expected_source = (
            "fresh-exact-head-archive"
            if fields["commit_bound"] == "true"
            else "target/release/anubis"
        )
        if fields.get("build_mode") != expected_build_mode:
            raise SystemExit(
                f"versioned pin metadata build_mode is inconsistent: {path}"
            )
        if fields.get("source") != expected_source:
            raise SystemExit(f"versioned pin metadata source is inconsistent: {path}")
    return {**receipt, "path": str(path), "fields": fields}


def meta_receipt(
    path: Path,
    binary_sha256: str,
    expected_pin: str,
    *,
    require_modern_fields: bool,
) -> dict[str, Any]:
    raw, receipt = open_stable_regular(
        path,
        "pin metadata",
        require_nonwritable=True,
        capture_bytes=True,
    )
    assert raw is not None
    return validate_meta_receipt(
        path,
        raw,
        receipt,
        binary_sha256,
        expected_pin,
        require_modern_fields=require_modern_fields,
    )


def pin_identity_receipts(
    root: Path,
    pin: Path,
    *,
    expected_binary_sha256: str | None,
    expected_meta_sha256: str | None,
    require_modern_meta: bool,
) -> tuple[dict[str, Any], dict[str, Any]]:
    pin_dir = (root / "vm/pins").resolve(strict=True)
    if pin.parent != pin_dir:
        raise SystemExit(f"pin is outside the repository pin directory: {pin}")
    binary = pin_receipt(pin)
    if (
        expected_binary_sha256 is not None
        and binary["sha256"] != expected_binary_sha256
    ):
        raise SystemExit(
            "old pin sha256 does not match --expected-old-sha256: "
            f"observed={binary['sha256']} expected={expected_binary_sha256}"
        )
    expected_pin = pin.relative_to(root).as_posix()
    metadata = meta_receipt(
        Path(str(pin) + ".meta"),
        binary["sha256"],
        expected_pin,
        require_modern_fields=require_modern_meta,
    )
    fields = metadata["fields"]
    legacy_name = f"anubis-{binary['sha256'][:12]}"
    source_name = f"{legacy_name}-src-{fields['src_tree'][:12]}"
    if fields.get("pin_schema") == CURRENT_PIN_SCHEMA:
        expected_name = (
            f"{source_name}-release"
            if fields.get("commit_bound") == "true"
            else source_name
        )
    else:
        expected_name = legacy_name
    if pin.name != expected_name:
        raise SystemExit(
            f"pin basename does not match its binary/source identity: {pin}"
        )
    if expected_meta_sha256 is not None and metadata["sha256"] != expected_meta_sha256:
        raise SystemExit(
            "old pin metadata sha256 does not match --expected-old-meta-sha256: "
            f"observed={metadata['sha256']} expected={expected_meta_sha256}"
        )
    return binary, metadata


def pin_identity_snapshots(
    root: Path,
    pin: Path,
    snapshot_dir: Path,
    *,
    expected_binary_sha256: str | None,
    expected_meta_sha256: str | None,
    require_modern_meta: bool,
) -> tuple[dict[str, Any], dict[str, Any], Path, dict[str, Any]]:
    """Validate a pin identity from one stable binary/meta snapshot pair."""
    pin_dir = (root / "vm/pins").resolve(strict=True)
    if pin.parent != pin_dir:
        raise SystemExit(f"pin is outside the repository pin directory: {pin}")

    binary_snapshot = snapshot_dir / pin.name
    _, binary_source, binary_copy = stable_snapshot_regular(
        pin,
        binary_snapshot,
        "pin",
        require_executable=True,
        require_nonwritable=True,
        snapshot_executable=True,
    )
    binary = validate_pin_receipt(pin, binary_source)
    if (
        expected_binary_sha256 is not None
        and binary["sha256"] != expected_binary_sha256
    ):
        raise SystemExit(
            "old pin sha256 does not match --expected-old-sha256: "
            f"observed={binary['sha256']} expected={expected_binary_sha256}"
        )

    metadata_path = Path(str(pin) + ".meta")
    metadata_snapshot = Path(str(binary_snapshot) + ".meta")
    metadata_raw, metadata_source, metadata_copy = stable_snapshot_regular(
        metadata_path,
        metadata_snapshot,
        "pin metadata",
        require_nonwritable=True,
        snapshot_executable=False,
        capture_bytes=True,
    )
    assert metadata_raw is not None
    expected_pin = pin.relative_to(root).as_posix()
    metadata = validate_meta_receipt(
        metadata_path,
        metadata_raw,
        metadata_source,
        binary["sha256"],
        expected_pin,
        require_modern_fields=require_modern_meta,
    )

    fields = metadata["fields"]
    legacy_name = f"anubis-{binary['sha256'][:12]}"
    source_name = f"{legacy_name}-src-{fields['src_tree'][:12]}"
    if fields.get("pin_schema") == CURRENT_PIN_SCHEMA:
        expected_name = (
            f"{source_name}-release"
            if fields.get("commit_bound") == "true"
            else source_name
        )
    else:
        expected_name = legacy_name
    if pin.name != expected_name:
        raise SystemExit(
            f"pin basename does not match its binary/source identity: {pin}"
        )
    if expected_meta_sha256 is not None and metadata["sha256"] != expected_meta_sha256:
        raise SystemExit(
            "old pin metadata sha256 does not match --expected-old-meta-sha256: "
            f"observed={metadata['sha256']} expected={expected_meta_sha256}"
        )

    snapshots = {
        "binary": compact_snapshot_receipt(binary_copy),
        "metadata": compact_snapshot_receipt(metadata_copy),
    }
    return binary, metadata, binary_snapshot, snapshots


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=True,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("ascii")


def duplicate_rejecting_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def source_manifest_rows(
    manifest: dict[str, Any],
    metadata: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    """Validate a full manifest and bind its exact rows to current pin metadata."""
    required = {
        "schema",
        "policy_path",
        "policy_schema",
        "policy_sha256",
        "count",
        "list_sha256",
        "tree_sha256",
        "rows",
    }
    if set(manifest) != required:
        raise SystemExit(
            "source manifest keys mismatch: "
            f"missing={sorted(required - set(manifest))} unknown={sorted(set(manifest) - required)}"
        )
    if manifest["schema"] != CURRENT_MANIFEST_SCHEMA:
        raise SystemExit(f"unsupported source manifest schema: {manifest['schema']!r}")
    if not isinstance(manifest["policy_path"], str) or not manifest["policy_path"]:
        raise SystemExit("source manifest policy_path must be a non-empty string")
    if not isinstance(manifest["policy_schema"], str) or not manifest["policy_schema"]:
        raise SystemExit("source manifest policy_schema must be a non-empty string")
    for key in ("policy_sha256", "list_sha256", "tree_sha256"):
        if not isinstance(manifest[key], str) or not HEX64.fullmatch(manifest[key]):
            raise SystemExit(f"source manifest {key} must be 64 lowercase hex")
    if not isinstance(manifest["count"], int) or isinstance(manifest["count"], bool):
        raise SystemExit("source manifest count must be an integer")
    rows = manifest["rows"]
    if not isinstance(rows, list) or manifest["count"] != len(rows) or not rows:
        raise SystemExit("source manifest count does not equal its non-empty row list")

    index: dict[str, dict[str, Any]] = {}
    ordered_paths: list[str] = []
    tree = hashlib.sha256()
    for row_number, row in enumerate(rows, start=1):
        if not isinstance(row, dict) or set(row) != {"path", "sha256", "executable"}:
            raise SystemExit(f"source manifest row {row_number} has malformed keys")
        relative = row["path"]
        if not isinstance(relative, str) or not relative:
            raise SystemExit(f"source manifest row {row_number} has an invalid path")
        pure = PurePosixPath(relative)
        if (
            "\\" in relative
            or "\x00" in relative
            or pure.is_absolute()
            or pure.as_posix() != relative
            or relative == "."
            or ".." in pure.parts
        ):
            raise SystemExit(
                f"source manifest row {row_number} path is not canonical: {relative!r}"
            )
        if relative in index:
            raise SystemExit(f"source manifest contains duplicate path: {relative}")
        if not isinstance(row["sha256"], str) or not HEX64.fullmatch(row["sha256"]):
            raise SystemExit(f"source manifest row {row_number} has an invalid sha256")
        if not isinstance(row["executable"], bool):
            raise SystemExit(
                f"source manifest row {row_number} has a non-boolean executable bit"
            )
        index[relative] = row
        ordered_paths.append(relative)
        tree.update(canonical_json_bytes(row) + b"\n")

    if ordered_paths != sorted(ordered_paths):
        raise SystemExit("source manifest rows are not sorted deterministically")
    list_digest = hashlib.sha256(
        json.dumps(ordered_paths, ensure_ascii=True, separators=(",", ":")).encode(
            "ascii"
        )
    ).hexdigest()
    if manifest["list_sha256"] != list_digest:
        raise SystemExit("source manifest list_sha256 does not match its rows")
    if manifest["tree_sha256"] != tree.hexdigest():
        raise SystemExit("source manifest tree_sha256 does not match its rows")

    fields = metadata["fields"]
    expected = {
        "schema": fields["manifest_schema"],
        "policy_sha256": fields["policy_sha256"],
        "count": int(fields["src_count"]),
        "list_sha256": fields["src_list_sha256"],
        "tree_sha256": fields["src_tree"],
    }
    observed = {key: manifest[key] for key in expected}
    if observed != expected:
        raise SystemExit(
            "full source manifest does not match new pin metadata: "
            f"observed={observed} expected={expected}"
        )
    return index


def compact_command_receipt(receipt: dict[str, Any]) -> dict[str, Any]:
    stdout = receipt["stdout"]
    stderr = receipt["stderr"]
    return {
        "argv": receipt["argv"],
        "rc": receipt["rc"],
        "elapsed_seconds": receipt["elapsed_seconds"],
        "timeout": receipt["timeout"],
        "stdout_sha256": hashlib.sha256(stdout.encode()).hexdigest(),
        "stdout_size_bytes": len(stdout.encode()),
        "stderr": stderr,
    }


def capture_source_manifest(
    root: Path,
    helper: Path,
    metadata: dict[str, Any],
) -> tuple[dict[str, Any], dict[str, dict[str, Any]], dict[str, Any]]:
    receipt = run_capture(
        isolated_python_command(helper, "--root", str(root), "--field", "json"),
        root,
        env=trusted_git_environment(),
    )
    if receipt["rc"] != 0 or receipt["timeout"]:
        raise SystemExit(f"source manifest helper failed: {receipt}")
    try:
        parsed = json.loads(
            receipt["stdout"], object_pairs_hook=duplicate_rejecting_object
        )
    except (json.JSONDecodeError, ValueError) as exc:
        raise SystemExit(
            f"source manifest helper returned malformed JSON: {exc}"
        ) from exc
    if not isinstance(parsed, dict):
        raise SystemExit("source manifest helper returned a non-object root")
    rows = source_manifest_rows(parsed, metadata)
    return parsed, rows, compact_command_receipt(receipt)


def snapshot_authoritative_fixtures(
    root: Path,
    files: list[str],
    manifest_rows: dict[str, dict[str, Any]],
    snapshot_root: Path,
) -> tuple[list[tuple[str, Path]], dict[str, Any]]:
    snapshots: list[tuple[str, Path]] = []
    receipt_rows: list[dict[str, Any]] = []
    for relative in files:
        pure = PurePosixPath(relative)
        if (
            pure.is_absolute()
            or pure.as_posix() != relative
            or ".." in pure.parts
            or not (
                relative.startswith("examples/")
                or relative.startswith("tests/fixtures/")
            )
        ):
            raise SystemExit(
                f"fixture path is not canonical or authoritative: {relative!r}"
            )
        manifest_row = manifest_rows.get(relative)
        if manifest_row is None:
            raise SystemExit(
                f"authoritative fixture is absent from new pin source manifest: {relative}"
            )
        source = root / relative
        destination = snapshot_root / relative
        _, source_receipt, snapshot_receipt = stable_snapshot_regular(
            source,
            destination,
            f"authoritative fixture {relative}",
            snapshot_executable=manifest_row["executable"],
        )
        for label, observed in (
            ("source", source_receipt),
            ("snapshot", snapshot_receipt),
        ):
            if (
                observed["sha256"] != manifest_row["sha256"]
                or observed["executable"] != manifest_row["executable"]
            ):
                raise SystemExit(
                    f"{label} fixture does not match new pin source manifest: {relative}"
                )
        snapshots.append((relative, destination))
        receipt_rows.append(
            {
                "path": relative,
                "sha256": snapshot_receipt["sha256"],
                "executable": snapshot_receipt["executable"],
            }
        )

    rows_digest = hashlib.sha256()
    for row in receipt_rows:
        rows_digest.update(canonical_json_bytes(row) + b"\n")
    return snapshots, {
        "count": len(receipt_rows),
        "rows_sha256": rows_digest.hexdigest(),
        "first_path": receipt_rows[0]["path"] if receipt_rows else None,
        "last_path": receipt_rows[-1]["path"] if receipt_rows else None,
    }


def verify_authoritative_fixture_snapshots(
    fixtures: list[tuple[str, Path]],
    manifest_rows: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    receipt_rows: list[dict[str, Any]] = []
    for relative, snapshot in fixtures:
        manifest_row = manifest_rows[relative]
        _, receipt = open_stable_regular(
            snapshot,
            f"authoritative fixture snapshot {relative}",
            require_nonwritable=True,
        )
        if (
            receipt["sha256"] != manifest_row["sha256"]
            or receipt["executable"] != manifest_row["executable"]
        ):
            raise SystemExit(
                f"fixture snapshot changed after measurement or no longer matches manifest: {relative}"
            )
        receipt_rows.append(
            {
                "path": relative,
                "sha256": receipt["sha256"],
                "executable": receipt["executable"],
            }
        )
    rows_digest = hashlib.sha256()
    for row in receipt_rows:
        rows_digest.update(canonical_json_bytes(row) + b"\n")
    return {
        "count": len(receipt_rows),
        "rows_sha256": rows_digest.hexdigest(),
        "first_path": receipt_rows[0]["path"] if receipt_rows else None,
        "last_path": receipt_rows[-1]["path"] if receipt_rows else None,
    }


def compact_source_manifest_receipt(manifest: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema": manifest["schema"],
        "policy_sha256": manifest["policy_sha256"],
        "count": manifest["count"],
        "list_sha256": manifest["list_sha256"],
        "tree_sha256": manifest["tree_sha256"],
        "rows_sha256": hashlib.sha256(
            canonical_json_bytes(manifest["rows"])
        ).hexdigest(),
        "manifest_sha256": hashlib.sha256(canonical_json_bytes(manifest)).hexdigest(),
    }


def validate_output_path(root: Path, raw_out: Path) -> Path:
    """Require a non-aliased output path below the excluded repository out/ root."""
    out = Path(os.path.abspath(raw_out))
    output_root = root / "out"
    try:
        relative = out.relative_to(output_root)
    except ValueError as exc:
        raise SystemExit(
            f"--out must be below the repository's excluded output root: {output_root}"
        ) from exc
    if not relative.parts:
        raise SystemExit(
            f"--out must name a file below the excluded output root: {output_root}"
        )

    current = root
    components = out.relative_to(root).parts
    for index, component in enumerate(components):
        current /= component
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            break
        except OSError as exc:
            raise SystemExit(
                f"cannot inspect --out path component {current}: {exc}"
            ) from exc
        if stat.S_ISLNK(metadata.st_mode):
            raise SystemExit(
                f"--out path must not contain symlink components: {current}"
            )
        if index < len(components) - 1 and not stat.S_ISDIR(metadata.st_mode):
            raise SystemExit(f"--out parent component is not a directory: {current}")
    return out


def validate_output_root_exclusion(
    root: Path,
    manifest: dict[str, Any],
    manifest_rows: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    """Prove that out/ is an explicit directory exclusion in the pin-bound policy."""
    policy_relative = manifest["policy_path"]
    policy_row = manifest_rows.get(policy_relative)
    if policy_row is None:
        raise SystemExit(
            "pin-bound source manifest does not contain its manifest policy"
        )
    raw, receipt = open_stable_regular(
        root / policy_relative,
        "pin manifest policy",
        capture_bytes=True,
    )
    assert raw is not None
    if (
        receipt["sha256"] != policy_row["sha256"]
        or receipt["executable"] != policy_row["executable"]
        or receipt["sha256"] != manifest["policy_sha256"]
    ):
        raise SystemExit(
            "pin manifest policy snapshot does not match the new pin-bound manifest"
        )
    try:
        policy = json.loads(raw, object_pairs_hook=duplicate_rejecting_object)
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as exc:
        raise SystemExit(f"cannot parse pin-bound manifest policy: {exc}") from exc
    if not isinstance(policy, dict):
        raise SystemExit("pin-bound manifest policy root must be an object")
    excluded = policy.get("excluded_top_level_entries")
    output_spec = excluded.get("out") if isinstance(excluded, dict) else None
    if not isinstance(output_spec, dict) or output_spec.get("kind") != "directory":
        raise SystemExit(
            "pin-bound manifest policy does not explicitly exclude out/ as a directory"
        )
    if "out" in policy.get("roots", []) or "out" in policy.get("files", []):
        raise SystemExit("pin-bound manifest policy also binds the output root")
    return compact_snapshot_receipt(receipt)


def main() -> int:
    python_runtime_open = require_isolated_python_runtime()
    parser = argparse.ArgumentParser(
        usage=(
            "/usr/bin/python3 -I -B scripts/phase1_verdict_diff.py "
            "--old PIN --new PIN --expected-old-sha256 SHA256 "
            "--expected-old-meta-sha256 SHA256 --root ROOT --out OUT [options]"
        ),
        epilog=(
            "Non-isolated `python3 scripts/phase1_verdict_diff.py ...` is refused because "
            "PYTHONPATH, user-site, and sitecustomize run before producer logic."
        ),
    )
    parser.add_argument("--old", required=True, type=Path)
    parser.add_argument("--new", required=True, type=Path)
    parser.add_argument("--expected-old-sha256", required=True)
    parser.add_argument("--expected-old-meta-sha256", required=True)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--workers", default=4, type=int)
    parser.add_argument("--timeout", default=90, type=int)
    parser.add_argument("--expected-count", default=921, type=int)
    args = parser.parse_args()

    started_utc = dt.datetime.now(dt.timezone.utc)
    started_mono = time.monotonic()
    root = args.root.resolve(strict=True)
    old = resolve_pin_argument(args.old, "--old")
    new = resolve_pin_argument(args.new, "--new")
    helper = (root / "scripts/lib/native_corpus_inventory.py").resolve(strict=True)
    source_manifest_helper = root / "scripts/lib/pin_manifest.py"
    producer = Path(__file__).resolve(strict=True)

    if not 1 <= args.workers <= 4:
        raise SystemExit("workers must be in [1,4]")
    if args.timeout != 90:
        raise SystemExit(
            "Phase-1 final diff requires exactly 90 seconds per invocation"
        )
    if args.expected_count != 921:
        raise SystemExit("Phase-1 final diff requires expected count 921")
    if not re.fullmatch(r"[0-9a-f]{64}", args.expected_old_sha256):
        raise SystemExit("--expected-old-sha256 must be 64 lowercase hex")
    if not re.fullmatch(r"[0-9a-f]{64}", args.expected_old_meta_sha256):
        raise SystemExit("--expected-old-meta-sha256 must be 64 lowercase hex")
    if old == new:
        raise SystemExit("old and new pins must differ")
    out = validate_output_path(root, args.out)
    if out.exists() or out.is_symlink():
        raise SystemExit(f"refusing to overwrite existing output: {out}")

    current_file = root / "vm/pins/CURRENT"
    current_open, current_receipt_open = read_current_receipt(root, current_file)
    expected_current_value = new.relative_to(root).as_posix()
    if current_open != new or current_receipt_open["value"] != expected_current_value:
        raise SystemExit(
            "CURRENT does not exactly name --new: "
            f"current={current_receipt_open['value']} new={expected_current_value}"
        )
    system_bash, system_bash_open = validated_system_bash()
    pin_verify_environment_open, pin_verify_environment_contract_open = (
        pin_verification_environment()
    )

    with tempfile.TemporaryDirectory(
        prefix="anubis-phase1-verdict-diff-"
    ) as raw_snapshot_root:
        snapshot_root = Path(raw_snapshot_root).resolve(strict=True)
        snapshot_root.chmod(0o700)
        old_open, old_meta_open, old_snapshot, old_snapshots = pin_identity_snapshots(
            root,
            old,
            snapshot_root / "pins/old",
            expected_binary_sha256=args.expected_old_sha256,
            expected_meta_sha256=args.expected_old_meta_sha256,
            require_modern_meta=False,
        )
        new_open, new_meta_open, new_snapshot, new_snapshots = pin_identity_snapshots(
            root,
            new,
            snapshot_root / "pins/new",
            expected_binary_sha256=None,
            expected_meta_sha256=None,
            require_modern_meta=True,
        )
        checker_z3_snapshot, checker_z3_binding_open = snapshot_checker_z3(
            root,
            snapshot_root / "toolchain/bin/z3",
        )
        source_manifest_helper = source_manifest_helper.resolve(strict=True)

        verify_open = run_pin_verification(
            root,
            system_bash,
            environment=pin_verify_environment_open,
        )
        if verify_open["rc"] != 0 or verify_open["timeout"]:
            raise SystemExit(f"opening pin verification failed: {verify_open}")

        inventory_open, inventory_receipt_open = read_inventory(root, helper)
        files = inventory_open["files"]
        if inventory_open["count"] != args.expected_count:
            raise SystemExit(
                f"inventory count {inventory_open['count']} != expected {args.expected_count}"
            )
        manifest_sha = hashlib.sha256(("\n".join(files) + "\n").encode()).hexdigest()
        source_manifest_open, manifest_rows, source_manifest_command_open = (
            capture_source_manifest(root, source_manifest_helper, new_meta_open)
        )
        manifest_inventory = sorted(
            relative
            for relative in manifest_rows
            if relative.endswith(".anb")
            and (
                relative.startswith("examples/")
                or relative.startswith("tests/fixtures/")
            )
        )
        if files != manifest_inventory:
            raise SystemExit(
                "native corpus inventory does not exactly match the new pin-bound source manifest"
            )
        output_policy_receipt = validate_output_root_exclusion(
            root,
            source_manifest_open,
            manifest_rows,
        )
        fixtures, fixture_snapshot_receipt = snapshot_authoritative_fixtures(
            root,
            files,
            manifest_rows,
            snapshot_root / "corpus",
        )
        checker_environment_open, checker_environment_contract_open = (
            checker_child_environment(
                checker_z3_snapshot,
                checker_z3_binding_open["snapshot"],
            )
        )
        checker_environment_probe = run_capture(
            ["z3", "--version"],
            root,
            env=checker_environment_open,
        )
        if (
            checker_environment_probe["rc"] != 0
            or checker_environment_probe["timeout"]
            or not checker_environment_probe["stdout"].startswith("Z3 version 4.15.4")
        ):
            raise SystemExit(
                "curated checker environment does not resolve the pinned Z3 4.15.4: "
                f"{checker_environment_probe}"
            )

        toolchain_commands = [
            ["sw_vers"],
            ["uname", "-m"],
            ["rustc", "--version", "--verbose"],
            ["cargo", "--version", "--verbose"],
            ["lean", "--version"],
            ["lake", "--version"],
            ["z3", "--version"],
            ["tart", "--version"],
        ]
        toolchain = [run_capture(command, root) for command in toolchain_commands]
        if any(item["rc"] != 0 or item["timeout"] for item in toolchain):
            raise SystemExit("one or more required machine/toolchain probes failed")

        def compare(item: tuple[str, Path]) -> dict[str, Any]:
            rel, fixture = item
            before = invoke(
                old_snapshot,
                fixture,
                args.timeout,
                environment=checker_environment_open,
                z3_snapshot=checker_z3_snapshot,
                z3_snapshot_receipt=checker_z3_binding_open["snapshot"],
            )
            after = invoke(
                new_snapshot,
                fixture,
                args.timeout,
                environment=checker_environment_open,
                z3_snapshot=checker_z3_snapshot,
                z3_snapshot_receipt=checker_z3_binding_open["snapshot"],
            )
            timeout_seen = before["timeout"] or after["timeout"]
            acceptance_flip = (
                not timeout_seen
                and before["class"] in {"ACCEPT", "REJECT"}
                and after["class"] in {"ACCEPT", "REJECT"}
                and before["class"] != after["class"]
            )
            return {
                "fixture": rel,
                "old": before,
                "new": after,
                "acceptance_flip": acceptance_flip,
                "return_code_changed": before["rc"] != after["rc"],
            }

        with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
            rows = list(pool.map(compare, fixtures))

        acceptance_flips = [row for row in rows if row["acceptance_flip"]]
        timeouts = [
            row for row in rows if row["old"]["timeout"] or row["new"]["timeout"]
        ]
        rc_changes = [row for row in rows if row["return_code_changed"]]
        fixture_snapshot_receipt_close = verify_authoritative_fixture_snapshots(
            fixtures,
            manifest_rows,
        )

        inventory_close, inventory_receipt_close = read_inventory(root, helper)
        source_manifest_close, _, source_manifest_command_close = (
            capture_source_manifest(
                root,
                source_manifest_helper,
                new_meta_open,
            )
        )
        pin_verify_environment_close, pin_verify_environment_contract_close = (
            pin_verification_environment()
        )
        verify_close = run_pin_verification(
            root,
            system_bash,
            environment=pin_verify_environment_close,
        )
        current_close, current_receipt_close = read_current_receipt(root, current_file)
        _, system_bash_close = validated_system_bash()
        python_runtime_close = require_isolated_python_runtime()
        old_close, old_meta_close = pin_identity_receipts(
            root,
            old,
            expected_binary_sha256=args.expected_old_sha256,
            expected_meta_sha256=args.expected_old_meta_sha256,
            require_modern_meta=False,
        )
        new_close, new_meta_close = pin_identity_receipts(
            root,
            new,
            expected_binary_sha256=None,
            expected_meta_sha256=None,
            require_modern_meta=True,
        )
        _, old_snapshot_close = open_stable_regular(
            old_snapshot,
            "old binary snapshot",
            require_executable=True,
            require_nonwritable=True,
        )
        _, new_snapshot_close = open_stable_regular(
            new_snapshot,
            "new binary snapshot",
            require_executable=True,
            require_nonwritable=True,
        )
        checker_z3_binding_close = close_checker_z3_binding(
            root,
            Path(checker_z3_binding_open["source"]["path"]),
            checker_z3_snapshot,
        )
        checker_environment_close, checker_environment_contract_close = (
            checker_child_environment(
                checker_z3_snapshot,
                checker_z3_binding_open["snapshot"],
            )
        )
        checker_environment_probe_close = run_capture(
            ["z3", "--version"],
            root,
            env=checker_environment_close,
        )
        output_close = validate_output_path(root, out)

        invariants = {
            "opening_pin_verify_passed": verify_open["rc"] == 0
            and not verify_open["timeout"],
            "closing_pin_verify_passed": verify_close["rc"] == 0
            and not verify_close["timeout"],
            "current_unchanged_and_names_new": current_open == current_close == new,
            "current_receipt_unchanged": current_receipt_open == current_receipt_close,
            "system_bash_unchanged": system_bash_open == system_bash_close,
            "pin_verification_environment_unchanged": pin_verify_environment_open
            == pin_verify_environment_close
            and pin_verify_environment_contract_open
            == pin_verify_environment_contract_close,
            "isolated_python_runtime_unchanged": python_runtime_open
            == python_runtime_close,
            "output_path_remained_below_excluded_root": output_close == out,
            "inventory_unchanged": inventory_open == inventory_close,
            "inventory_bound_to_source_manifest": files == manifest_inventory,
            "source_manifest_unchanged": source_manifest_open == source_manifest_close,
            "inventory_count_exact": len(rows)
            == inventory_open["count"]
            == args.expected_count,
            "fixture_snapshot_count_exact": fixture_snapshot_receipt["count"]
            == len(files),
            "fixture_snapshots_unchanged": fixture_snapshot_receipt
            == fixture_snapshot_receipt_close,
            "row_paths_unique": len({row["fixture"] for row in rows}) == len(rows),
            "row_paths_match_inventory": [row["fixture"] for row in rows] == files,
            "old_pin_unchanged": old_open == old_close,
            "new_pin_unchanged": new_open == new_close,
            "old_meta_unchanged": old_meta_open == old_meta_close,
            "new_meta_unchanged": new_meta_open == new_meta_close,
            "old_binary_snapshot_unchanged": compact_snapshot_receipt(
                old_snapshot_close
            )
            == old_snapshots["binary"],
            "new_binary_snapshot_unchanged": compact_snapshot_receipt(
                new_snapshot_close
            )
            == new_snapshots["binary"],
            "checker_environment_unchanged": checker_environment_open
            == checker_environment_close
            and checker_environment_contract_open == checker_environment_contract_close,
            "checker_environment_pinned_z3_available": checker_environment_probe["rc"]
            == 0
            and not checker_environment_probe["timeout"]
            and checker_environment_probe["stdout"].startswith("Z3 version 4.15.4"),
            "checker_z3_binding_unchanged": compact_checker_z3_binding(
                checker_z3_binding_open
            )
            == compact_checker_z3_binding(checker_z3_binding_close),
            "checker_z3_closing_probe_passed": checker_environment_probe_close["rc"]
            == 0
            and not checker_environment_probe_close["timeout"]
            and checker_environment_probe_close["stdout"]
            == checker_environment_probe["stdout"],
            "zero_timeouts": len(timeouts) == 0,
            "zero_acceptance_flips": len(acceptance_flips) == 0,
        }
        verdict = "PASS" if all(invariants.values()) else "FAIL"
        finished_utc = dt.datetime.now(dt.timezone.utc)

        report = {
            "schema": "anubis.phase1.verdict-diff.v2",
            "verdict": verdict,
            "started_utc": started_utc.isoformat().replace("+00:00", "Z"),
            "finished_utc": finished_utc.isoformat().replace("+00:00", "Z"),
            "elapsed_seconds": round(time.monotonic() - started_mono, 3),
            "root": str(root),
            "producer": {"path": str(producer), "sha256": sha256(producer)},
            "inventory_helper": {"path": str(helper), "sha256": sha256(helper)},
            "source_manifest_helper": {
                "path": str(source_manifest_helper),
                "sha256": sha256(source_manifest_helper),
            },
            "source_tree_hash": new_meta_open["fields"]["src_tree"],
            "old": old_open,
            "new": new_open,
            "expected_old_sha256": args.expected_old_sha256,
            "expected_old_meta_sha256": args.expected_old_meta_sha256,
            "old_meta": old_meta_open,
            "new_meta": new_meta_open,
            "snapshots": {
                "old_pin": old_snapshots,
                "new_pin": new_snapshots,
                "fixtures": fixture_snapshot_receipt,
                "fixtures_close": fixture_snapshot_receipt_close,
                "source_manifest": compact_source_manifest_receipt(
                    source_manifest_open
                ),
                "output_policy": output_policy_receipt,
            },
            "closing_identity": {
                "old": old_close,
                "new": new_close,
                "old_meta": old_meta_close,
                "new_meta": new_meta_close,
                "current": current_receipt_close,
            },
            "scope": {
                "roots": ["examples", "tests/fixtures"],
                "examples": sum(rel.startswith("examples/") for rel in files),
                "tests_fixtures": sum(
                    rel.startswith("tests/fixtures/") for rel in files
                ),
                "total": len(files),
                "manifest_sha256": manifest_sha,
            },
            "workers": args.workers,
            "timeout_seconds_per_invocation": args.timeout,
            "checker_environment": checker_environment_contract_open,
            "checker_environment_probe": checker_environment_probe,
            "checker_z3_binding": {
                "opening": checker_z3_binding_open,
                "closing": checker_z3_binding_close,
                "closing_version_probe": checker_environment_probe_close,
            },
            "machine": {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "cpu_count": os.cpu_count(),
                "toolchain_commands": toolchain,
            },
            "pin_verify_open": verify_open,
            "pin_verify_close": verify_close,
            "pin_verify_shell": {
                "opening": system_bash_open,
                "closing": system_bash_close,
            },
            "pin_verify_environment": {
                "opening": pin_verify_environment_contract_open,
                "closing": pin_verify_environment_contract_close,
            },
            "python_runtime": {
                "opening": python_runtime_open,
                "closing": python_runtime_close,
            },
            "current": current_receipt_open,
            "inventory_receipt_open": inventory_receipt_open,
            "inventory_receipt_close": inventory_receipt_close,
            "source_manifest_command_open": source_manifest_command_open,
            "source_manifest_command_close": source_manifest_command_close,
            "invariants": invariants,
            "acceptance_flips": acceptance_flips,
            "acceptance_flip_count": len(acceptance_flips),
            "timeouts": timeouts,
            "timeout_count": len(timeouts),
            "return_code_changes": rc_changes,
            "return_code_change_count": len(rc_changes),
            "rows": rows,
        }

        publish_report_no_clobber(out, report)
    print(
        "VERDICT_DIFF_V2 "
        f"verdict={verdict} total={len(rows)} flips={len(acceptance_flips)} "
        f"timeouts={len(timeouts)} rc_changes={len(rc_changes)} out={out}"
    )
    return 0 if verdict == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
