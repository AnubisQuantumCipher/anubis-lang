#!/bin/bash -p

# Bash processes BASH_ENV and imported functions before it reads this file. A
# privileged-mode interpreter is therefore part of the trust boundary, not a
# cleanup detail that can be repaired later. Direct execution gets `-p` from
# the shebang. Preserve legacy unprivileged read-only callers by relaunching
# once with `-p`; mutation requires a privileged entrypoint. Always refuse a
# BASH_ENV-bearing unprivileged shell: that startup file has already executed
# and cannot be made safe retroactively.
if [[ "$-" != *p* ]]; then
  if [[ -n "${BASH_ENV:-}" ]]; then
    /bin/echo "PIN_SHELL_UNTRUSTED: BASH_ENV requires direct execution or /bin/bash -p" >&2
    /usr/bin/false
  else
    case "${1:-}" in
      --current|--verify|--verify-release) /bin/bash -p "$0" "$@" ;;
      *)
        /bin/echo "PIN_SHELL_UNTRUSTED: publication requires direct execution or /bin/bash -p" >&2
        /usr/bin/false
        ;;
    esac
  fi
else
set -euo pipefail
IFS=$' \t\n'
unset CDPATH ENV

# Publish an immutable snapshot of the release binary and bind it to one exact
# source-manifest epoch. New pin identities include both the binary digest and
# source-tree digest, so identical executable bytes can never rebind an older
# pin's provenance after a docs, proof, gate, or fixture change.
#
#   scripts/publish_pin.sh                 technical pin (dirty source allowed)
#   scripts/publish_pin.sh --release       clean commit-bound release pin
#   scripts/publish_pin.sh --current       resolve CURRENT without mutation
#   scripts/publish_pin.sh --verify        verify bytes and current source epoch
#   scripts/publish_pin.sh --verify-release
#                                           additionally require the exact clean
#                                           commit recorded by --release

export PATH="/usr/bin:/bin:/usr/sbin:/sbin"
script_path="${BASH_SOURCE[0]}"
case "$script_path" in
  */*) script_dir="${script_path%/*}" ;;
  *) script_dir=. ;;
esac
REPO_ROOT="$(cd "$script_dir/.." && pwd -P)"
cd "$REPO_ROOT"

# Pin provenance must not depend on caller-prepended command shims. Keep the
# operating-system tools first and address the Python/tar interpreters by their
# immutable platform paths. Cargo is intentionally resolved from rustup's
# conventional per-user toolchain location only when release mode builds.
PYTHON_BIN=/usr/bin/python3
TAR_BIN=/usr/bin/bsdtar
ENV_BIN=/usr/bin/env
for trusted_tool in "$PYTHON_BIN" "$TAR_BIN" "$ENV_BIN"; do
  if [[ ! -f "$trusted_tool" || -L "$trusted_tool" || ! -x "$trusted_tool" ]]; then
    echo "PIN_TOOL_UNTRUSTED: required platform tool is unavailable: $trusted_tool" >&2
    exit 1
  fi
done
trusted_python() {
  # Isolated mode ignores PYTHONPATH, PYTHONHOME, user-site packages, and
  # sitecustomize; bytecode suppression keeps critical reads side-effect free.
  "$PYTHON_BIN" -I -B "$@"
}
OPERATOR_HOME="$(trusted_python -c 'import os,pwd; print(pwd.getpwuid(os.getuid()).pw_dir)')"
if [[ "$OPERATOR_HOME" != /* || ! -d "$OPERATOR_HOME" || -L "$OPERATOR_HOME" ]]; then
  echo "PIN_TOOL_UNTRUSTED: canonical operator home is unavailable" >&2
  exit 1
fi
CANONICAL_CARGO_HOME="$OPERATOR_HOME/.cargo"
CANONICAL_RUSTUP_HOME="$OPERATOR_HOME/.rustup"
CARGO_BIN="$CANONICAL_CARGO_HOME/bin/cargo"
CANONICAL_BUILD_PATH="/usr/bin:/bin:/usr/sbin:/sbin:$CANONICAL_CARGO_HOME/bin"
export PATH="$CANONICAL_BUILD_PATH"
export TMPDIR=/tmp

PIN_DIR="vm/pins"
CURRENT="$PIN_DIR/CURRENT"
SRC="target/release/anubis"
PIN_MANIFEST_POLICY="scripts/lib/pin_manifest_policy.json"
PIN_SCHEMA="anubis.binary-pin.v2"

usage() {
  echo "usage: scripts/publish_pin.sh [--release|--current|--verify|--verify-release]" >&2
  exit 2
}

MODE="publish"
case "${1:-}" in
  "") ;;
  --release) MODE="release" ;;
  --current) MODE="current" ;;
  --verify) MODE="verify" ;;
  --verify-release) MODE="verify-release" ;;
  *) usage ;;
esac
[[ $# -le 1 ]] || usage

resolve_trusted_git() {
  # Release provenance must not trust a PATH-prepended shim. Use only the OS command path and
  # require one absolute, executable, non-symlink Git binary.
  local candidate
  candidate="$(PATH=/usr/bin:/bin:/usr/sbin:/sbin command -v git 2>/dev/null || true)"
  if [[ "$candidate" != /* || ! -f "$candidate" || -L "$candidate" || ! -x "$candidate" ]]; then
    echo "PIN_GIT_UNTRUSTED: trusted system Git executable is unavailable" >&2
    return 1
  fi
  GIT_BIN="$candidate"
}

sanitize_git_environment() {
  # A trusted executable and `-C` still honor environment-directed repositories, indexes, object
  # stores, replacements, and config. None is valid for a pin rooted at this checkout.
  unset GIT_DIR GIT_WORK_TREE GIT_COMMON_DIR GIT_INDEX_FILE GIT_SHALLOW_FILE GIT_GRAFT_FILE
  unset GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE GIT_QUARANTINE_PATH
  unset GIT_REPLACE_REF_BASE GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT
  unset GIT_CEILING_DIRECTORIES GIT_DISCOVERY_ACROSS_FILESYSTEM GIT_EXEC_PATH
  export GIT_CONFIG_GLOBAL=/dev/null
  export GIT_CONFIG_SYSTEM=/dev/null
  export GIT_CONFIG_NOSYSTEM=1
  export GIT_ATTR_NOSYSTEM=1
  export GIT_NO_REPLACE_OBJECTS=1
  export GIT_OPTIONAL_LOCKS=0
}

reject_release_build_overrides() {
  # Release publication intentionally accepts only ANUBIS_RELEASE_BUILD_JOBS as
  # a build-shaping caller input. Cargo and rustup map many environment names
  # into config keys, and compiler wrappers/flags can change the resulting bytes
  # even when the cargo executable and source archive are fixed. Reject the
  # high-risk families explicitly so a poisoned invocation fails visibly; the
  # build itself additionally starts from `env -i`, which drops every other
  # uncurated caller variable.
  local release_env_var
  while IFS= read -r release_env_var; do
    case "$release_env_var" in
      RUSTUP_TOOLCHAIN|RUSTUP_DIST_SERVER|RUSTUP_UPDATE_ROOT|\
      RUSTC|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|RUSTC_BOOTSTRAP|\
      RUSTFLAGS|RUSTDOC|RUSTDOCFLAGS|CARGO_ENCODED_RUSTFLAGS|\
      CARGO_BUILD_*|CARGO_PROFILE_*|CARGO_TARGET_*)
        echo "PIN_RELEASE_BUILD_ENV_DENIED: $release_env_var must be unset" >&2
        return 1
        ;;
    esac
  done < <(compgen -e)
}

assert_no_parent_cargo_configuration() {
  local release_root="$1"
  trusted_python - "$release_root" <<'PY'
import os
import stat
import sys

release_root = os.path.abspath(sys.argv[1])
try:
    root_stat = os.lstat(release_root)
except OSError as exc:
    print(f"PIN_RELEASE_PARENT_CARGO_CONFIG: release root unavailable: {exc}", file=sys.stderr)
    raise SystemExit(1)
if not stat.S_ISDIR(root_stat.st_mode) or os.path.realpath(release_root) != release_root:
    print("PIN_RELEASE_PARENT_CARGO_CONFIG: release root must be a real directory", file=sys.stderr)
    raise SystemExit(1)

parent = os.path.dirname(release_root)
while True:
    for filename in ("config", "config.toml"):
        candidate = os.path.join(parent, ".cargo", filename)
        try:
            os.lstat(candidate)
        except FileNotFoundError:
            continue
        except OSError as exc:
            print(
                f"PIN_RELEASE_PARENT_CARGO_CONFIG: cannot inspect {candidate}: {exc}",
                file=sys.stderr,
            )
            raise SystemExit(1)
        print(
            f"PIN_RELEASE_PARENT_CARGO_CONFIG: unbound ancestor Cargo config: {candidate}",
            file=sys.stderr,
        )
        raise SystemExit(1)
    next_parent = os.path.dirname(parent)
    if next_parent == parent:
        break
    parent = next_parent
PY
}

GIT_BIN=""
resolve_trusted_git || exit 1
sanitize_git_environment

assert_repo_path_components() {
  local relative="$1"
  local allow_missing="${2:-0}"
  trusted_python - "$REPO_ROOT" "$relative" "$allow_missing" <<'PY'
import os
import stat
import sys
from pathlib import Path, PurePosixPath

root = Path(sys.argv[1])
relative = sys.argv[2]
allow_missing = sys.argv[3] == "1"
parts = PurePosixPath(relative).parts
if not parts or PurePosixPath(relative).is_absolute() or any(p in ("", ".", "..") for p in parts):
    print(f"PIN_PATH_INVALID: unsafe repository-relative path: {relative}", file=sys.stderr)
    raise SystemExit(1)
current = root
missing = False
for index, part in enumerate(parts):
    current /= part
    if missing:
        continue
    try:
        metadata = os.lstat(current)
    except FileNotFoundError:
        if allow_missing:
            missing = True
            continue
        print(f"PIN_PATH_INVALID: missing path component: {current}", file=sys.stderr)
        raise SystemExit(1)
    except OSError as exc:
        print(f"PIN_PATH_INVALID: cannot lstat {current}: {exc}", file=sys.stderr)
        raise SystemExit(1)
    if stat.S_ISLNK(metadata.st_mode):
        print(f"PIN_PATH_INVALID: symlink path component: {current}", file=sys.stderr)
        raise SystemExit(1)
    if index < len(parts) - 1 and not stat.S_ISDIR(metadata.st_mode):
        print(f"PIN_PATH_INVALID: intermediate component is not a directory: {current}", file=sys.stderr)
        raise SystemExit(1)
PY
}

pin_manifest() {
  trusted_python scripts/lib/pin_manifest.py \
    --root "$REPO_ROOT" \
    --policy "$PIN_MANIFEST_POLICY" \
    "$@"
}

json_field() {
  local path="$1"
  local field="$2"
  trusted_python - "$path" "$field" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as handle:
    value = json.load(handle)[sys.argv[2]]
print(value)
PY
}

manifest_summary_from_json() {
  local path="$1"
  trusted_python - "$path" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as handle:
    manifest = json.load(handle)
for label, key in (
    ("manifest_schema", "schema"),
    ("policy_sha256", "policy_sha256"),
    ("src_tree", "tree_sha256"),
    ("src_count", "count"),
    ("src_list_sha256", "list_sha256"),
):
    print(f"{label}: {manifest[key]}")
PY
}

stable_snapshot_regular() {
  local source="$1"
  local destination="$2"
  local require_executable="$3"
  trusted_python - "$source" "$destination" "$require_executable" <<'PY'
import hashlib
import os
import stat
import sys

source, destination, require_executable = sys.argv[1], sys.argv[2], sys.argv[3] == "1"
before = os.stat(source, follow_symlinks=False)
if not stat.S_ISREG(before.st_mode) or before.st_mode & 0o222:
    raise SystemExit("source must be a non-writable regular non-symlink file")
if require_executable and not before.st_mode & 0o111:
    raise SystemExit("source must be executable")
source_fd = os.open(source, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
try:
    opened = os.fstat(source_fd)
    destination_fd = os.open(
        destination,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
        0o500 if require_executable else 0o400,
    )
    digest = hashlib.sha256()
    try:
        while True:
            chunk = os.read(source_fd, 1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
            view = memoryview(chunk)
            while view:
                view = view[os.write(destination_fd, view):]
        os.fsync(destination_fd)
    finally:
        os.close(destination_fd)
    after = os.fstat(source_fd)
    path_after = os.stat(source, follow_symlinks=False)
    identity = lambda value: (
        value.st_dev,
        value.st_ino,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
        value.st_mode,
    )
    if identity(before) != identity(opened) or identity(opened) != identity(after) \
            or identity(after) != identity(path_after):
        raise RuntimeError("source changed or was replaced during snapshot")
    print(digest.hexdigest())
except Exception:
    try:
        os.unlink(destination)
    except FileNotFoundError:
        pass
    raise
finally:
    os.close(source_fd)
PY
}

stable_published_file_equals() {
  local published="$1"
  local expected="$2"
  local expected_mode="$3"
  trusted_python - "$REPO_ROOT/$published" "$expected" "$expected_mode" <<'PY'
import os
import stat
import sys

published, expected, raw_mode = sys.argv[1:]
required_mode = int(raw_mode, 8)
flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)


def identity(value):
    return (
        value.st_dev,
        value.st_ino,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
        value.st_mode,
    )


def open_stable(path, mode):
    try:
        before = os.stat(path, follow_symlinks=False)
        if (
            not stat.S_ISREG(before.st_mode)
            or stat.S_IMODE(before.st_mode) != mode
            or os.access(path, os.W_OK)
        ):
            raise SystemExit(1)
        fd = os.open(path, flags)
        opened = os.fstat(fd)
        if identity(before) != identity(opened):
            os.close(fd)
            raise SystemExit(1)
        return fd, before
    except OSError:
        raise SystemExit(1)


published_fd, published_before = open_stable(published, required_mode)
expected_fd, expected_before = open_stable(expected, required_mode)
try:
    while True:
        published_chunk = os.read(published_fd, 1024 * 1024)
        expected_chunk = os.read(expected_fd, 1024 * 1024)
        if published_chunk != expected_chunk:
            raise SystemExit(1)
        if not published_chunk:
            break
    published_after = os.fstat(published_fd)
    expected_after = os.fstat(expected_fd)
    try:
        published_path_after = os.stat(published, follow_symlinks=False)
        expected_path_after = os.stat(expected, follow_symlinks=False)
    except OSError:
        raise SystemExit(1)
    if identity(published_before) != identity(published_after) \
            or identity(published_after) != identity(published_path_after) \
            or identity(expected_before) != identity(expected_after) \
            or identity(expected_after) != identity(expected_path_after):
        raise SystemExit(1)
finally:
    os.close(published_fd)
    os.close(expected_fd)
PY
}

durable_publish_file() {
  local source="$1"
  local destination="$2"
  local mode="$3"
  trusted_python - "$source" "$REPO_ROOT/$destination" "$mode" <<'PY'
import os
import secrets
import stat
import sys

source, destination, raw_mode = sys.argv[1:]
mode = int(raw_mode, 8)
parent = os.path.dirname(destination)
name = os.path.basename(destination)
parent_fd = os.open(
    parent,
    os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0),
)
source_fd = os.open(source, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
temporary = f".publish-{os.getpid()}-{secrets.token_hex(16)}"
temporary_created = False
try:
    before = os.fstat(source_fd)
    if not stat.S_ISREG(before.st_mode):
        raise RuntimeError("publication source is not a regular file")
    temporary_fd = os.open(
        temporary,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
        0o600,
        dir_fd=parent_fd,
    )
    temporary_created = True
    try:
        while True:
            chunk = os.read(source_fd, 1024 * 1024)
            if not chunk:
                break
            view = memoryview(chunk)
            while view:
                view = view[os.write(temporary_fd, view):]
        os.fchmod(temporary_fd, mode)
        os.fsync(temporary_fd)
    finally:
        os.close(temporary_fd)
    after = os.fstat(source_fd)
    path_after = os.stat(source, follow_symlinks=False)
    identity = lambda value: (
        value.st_dev,
        value.st_ino,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
        value.st_mode,
    )
    if identity(before) != identity(after) or identity(after) != identity(path_after):
        raise RuntimeError("publication source changed or was replaced while copying")
    try:
        os.link(
            temporary,
            name,
            src_dir_fd=parent_fd,
            dst_dir_fd=parent_fd,
            follow_symlinks=False,
        )
    except FileExistsError:
        raise SystemExit(3)
    published = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    temporary_stat = os.stat(temporary, dir_fd=parent_fd, follow_symlinks=False)
    if not stat.S_ISREG(published.st_mode) or (published.st_dev, published.st_ino) != (
        temporary_stat.st_dev,
        temporary_stat.st_ino,
    ):
        raise RuntimeError("published destination identity mismatch")
    os.fsync(parent_fd)
finally:
    if temporary_created:
        try:
            os.unlink(temporary, dir_fd=parent_fd)
            os.fsync(parent_fd)
        except FileNotFoundError:
            pass
    os.close(source_fd)
    os.close(parent_fd)
PY
}

durable_replace_pointer() {
  local source="$1"
  local destination="$2"
  trusted_python - "$source" "$REPO_ROOT/$destination" <<'PY'
import os
import secrets
import stat
import sys

source, destination = sys.argv[1:]
parent = os.path.dirname(destination)
name = os.path.basename(destination)
parent_fd = os.open(
    parent,
    os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0),
)
source_fd = os.open(source, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
temporary = f".current-{os.getpid()}-{secrets.token_hex(16)}"
temporary_created = False
try:
    source_stat = os.fstat(source_fd)
    if not stat.S_ISREG(source_stat.st_mode):
        raise RuntimeError("CURRENT source is not regular")
    temporary_fd = os.open(
        temporary,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
        0o644,
        dir_fd=parent_fd,
    )
    temporary_created = True
    try:
        while True:
            chunk = os.read(source_fd, 65536)
            if not chunk:
                break
            view = memoryview(chunk)
            while view:
                view = view[os.write(temporary_fd, view):]
        os.fsync(temporary_fd)
    finally:
        os.close(temporary_fd)
    os.replace(temporary, name, src_dir_fd=parent_fd, dst_dir_fd=parent_fd)
    temporary_created = False
    published = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    if not stat.S_ISREG(published.st_mode):
        raise RuntimeError("published CURRENT is not regular")
    os.fsync(parent_fd)
finally:
    if temporary_created:
        try:
            os.unlink(temporary, dir_fd=parent_fd)
        except FileNotFoundError:
            pass
    os.close(source_fd)
    os.close(parent_fd)
PY
}

meta_value() {
  local path="$1"
  local key="$2"
  awk -F': ' -v wanted="$key" '$1 == wanted { print $2 }' "$path"
}

meta_field_count() {
  local path="$1"
  local key="$2"
  awk -F': ' -v wanted="$key" '$1 == wanted { count++ } END { print count + 0 }' "$path"
}

read_current_pin() {
  assert_repo_path_components "$PIN_DIR" 0
  if [[ ! -f "$CURRENT" || -L "$CURRENT" ]]; then
    echo "PIN_CURRENT_INVALID: CURRENT must be a regular non-symlink file" >&2
    return 1
  fi
  assert_repo_path_components "$CURRENT" 0
  local pin
  if ! pin="$(trusted_python - "$CURRENT" <<'PY'
import os
import re
import stat
import sys

path = sys.argv[1]
before = os.stat(path, follow_symlinks=False)
descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
try:
    opened = os.fstat(descriptor)
    raw = b""
    while True:
        chunk = os.read(descriptor, 4096)
        if not chunk:
            break
        raw += chunk
        if len(raw) > 4096:
            raise RuntimeError("CURRENT is unexpectedly large")
    after = os.fstat(descriptor)
finally:
    os.close(descriptor)
path_after = os.stat(path, follow_symlinks=False)
identity = lambda value: (
    value.st_dev,
    value.st_ino,
    value.st_size,
    value.st_mtime_ns,
    value.st_ctime_ns,
    value.st_mode,
)
if not stat.S_ISREG(opened.st_mode) or identity(before) != identity(opened) \
        or identity(opened) != identity(after) or identity(after) != identity(path_after):
    raise RuntimeError("CURRENT changed or was replaced while reading")
try:
    value = raw.decode("ascii")
except UnicodeDecodeError as exc:
    raise RuntimeError("CURRENT is not ASCII") from exc
if not re.fullmatch(r"vm/pins/anubis-[0-9a-f]{12}(?:-src-[0-9a-f]{12}(?:-release)?)?\n", value):
    raise RuntimeError("CURRENT does not contain exactly one canonical pin path")
print(value[:-1])
PY
)"; then
    echo "PIN_CURRENT_INVALID: CURRENT could not be read as one stable regular file" >&2
    return 1
  fi
  if [[ ! "$pin" =~ ^vm/pins/anubis-[0-9a-f]{12}(-src-[0-9a-f]{12}(-release)?)?$ ]]; then
    echo "PIN_CURRENT_INVALID: expected exactly one versioned vm/pins/anubis pin path" >&2
    return 1
  fi
  assert_repo_path_components "$pin" 0
  if [[ ! -f "$pin" || -L "$pin" || ! -x "$pin" || -w "$pin" ]]; then
    echo "PIN_FILE_INVALID: CURRENT must name a non-writable regular non-symlink executable" >&2
    return 1
  fi
  printf '%s\n' "$pin"
}

write_commit_manifest() {
  local commit="$1"
  local destination="$2"
  local archive_root="$3"
  mkdir -p "$archive_root"
  "$GIT_BIN" -C "$REPO_ROOT" archive --format=tar "$commit" > "$archive_root/source.tar"
  mkdir "$archive_root/tree"
  "$TAR_BIN" -xf "$archive_root/source.tar" -C "$archive_root/tree"
  trusted_python "$archive_root/tree/scripts/lib/pin_manifest.py" \
    --root "$archive_root/tree" \
    --policy scripts/lib/pin_manifest_policy.json \
    --field json > "$destination"
}

require_clean_commit_binding() {
  local live_manifest="$1"
  local expected_head="$2"
  local expected_tree="$3"
  local scratch="$4"
  local allow_current_update="${5:-0}"
  local actual_head actual_tree

  actual_head="$("$GIT_BIN" -C "$REPO_ROOT" rev-parse --verify 'HEAD^{commit}' 2>/dev/null)" || {
    echo "PIN_RELEASE_UNBOUND: HEAD is not a commit" >&2
    return 1
  }
  actual_tree="$("$GIT_BIN" -C "$REPO_ROOT" rev-parse --verify 'HEAD^{tree}' 2>/dev/null)" || {
    echo "PIN_RELEASE_UNBOUND: HEAD tree is unavailable" >&2
    return 1
  }
  if [[ "$actual_head" != "$expected_head" || "$actual_tree" != "$expected_tree" ]]; then
    echo "PIN_RELEASE_UNBOUND: HEAD changed during the release transaction" >&2
    return 1
  fi
  if ! "$GIT_BIN" -C "$REPO_ROOT" diff --cached --quiet --ignore-submodules --; then
    echo "PIN_RELEASE_DIRTY: staged changes forbid a release pin" >&2
    return 1
  fi
  if ! "$GIT_BIN" -C "$REPO_ROOT" diff --quiet --ignore-submodules --; then
    local changed resolved_current
    changed="$("$GIT_BIN" -C "$REPO_ROOT" diff --name-only --ignore-submodules --)"
    resolved_current="$(read_current_pin 2>/dev/null || true)"
    if [[ "$allow_current_update" != "1" || "$changed" != "$CURRENT" \
       || -z "$resolved_current" ]]; then
      echo "PIN_RELEASE_DIRTY: tracked changes other than the exact publication-owned CURRENT update forbid a release pin" >&2
      return 1
    fi
  fi

  write_commit_manifest "$expected_head" "$scratch/commit-manifest.json" "$scratch/archive"
  if ! cmp -s "$live_manifest" "$scratch/commit-manifest.json"; then
    echo "PIN_RELEASE_UNBOUND: filesystem manifest does not exactly equal the recorded commit" >&2
    return 1
  fi
}

snapshot_binary() {
  local source="$1"
  local destination="$2"
  trusted_python - "$source" "$destination" <<'PY'
import hashlib
import json
import os
import stat
import sys

source, destination = sys.argv[1:]
flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
fd = os.open(source, flags)
try:
    before = os.fstat(fd)
    if not stat.S_ISREG(before.st_mode) or not (before.st_mode & 0o111):
        raise RuntimeError("source binary must be a regular executable")
    digest = hashlib.sha256()
    out = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o555)
    try:
        while True:
            chunk = os.read(fd, 1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
            view = memoryview(chunk)
            while view:
                written = os.write(out, view)
                view = view[written:]
        os.fsync(out)
    finally:
        os.close(out)
    after = os.fstat(fd)
    path_after = os.stat(source, follow_symlinks=False)
    identity_before = (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns)
    identity_after = (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns)
    identity_path = (path_after.st_dev, path_after.st_ino, path_after.st_size, path_after.st_mtime_ns, path_after.st_ctime_ns)
    if identity_before != identity_after or identity_after != identity_path:
        raise RuntimeError("source binary changed or was replaced during snapshot")
    os.chmod(destination, 0o555)
    print(json.dumps({"sha256": digest.hexdigest(), "identity": list(identity_after)}, separators=(",", ":")))
except Exception:
    try:
        os.unlink(destination)
    except FileNotFoundError:
        pass
    raise
finally:
    os.close(fd)
PY
}

verify_pin() (
  local require_release="$1"
  local pin meta meta_snapshot scratch actual_summary actual_manifest
  local actual_sha initial_meta_sha closing_pin_sha closing_meta_sha closing_current
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/anubis-pin-verify.XXXXXX")"
  scratch="$(cd "$scratch" && pwd -P)"
  trap 'rm -rf "$scratch"' EXIT
  pin="$(read_current_pin)"
  meta="$pin.meta"
  if [[ ! -f "$meta" || -L "$meta" || -w "$meta" ]]; then
    echo "PIN_META_INVALID: metadata must be a non-writable regular non-symlink file" >&2
    return 1
  fi
  assert_repo_path_components "$meta" 0
  if ! actual_sha="$(stable_snapshot_regular "$pin" "$scratch/pin.snapshot" 1)"; then
    echo "PIN_FILE_INVALID: could not take a stable immutable pin snapshot" >&2
    return 1
  fi
  meta_snapshot="$scratch/meta.snapshot"
  if ! initial_meta_sha="$(stable_snapshot_regular "$meta" "$meta_snapshot" 0)"; then
    echo "PIN_META_INVALID: could not take a stable immutable metadata snapshot" >&2
    return 1
  fi

  local required_fields=(pin_schema pin sha256 source build_mode head head_tree commit_bound manifest_schema policy_sha256 src_tree src_count src_list_sha256)
  local field
  for field in "${required_fields[@]}"; do
    if [[ "$(meta_field_count "$meta_snapshot" "$field")" != "1" ]]; then
      echo "PIN_META_INVALID: expected exactly one $field field" >&2
      return 1
    fi
  done

  local recorded_schema recorded_pin recorded_sha recorded_source recorded_build_mode recorded_head recorded_head_tree
  local recorded_bound recorded_manifest_schema recorded_policy recorded_tree recorded_count recorded_list
  recorded_schema="$(meta_value "$meta_snapshot" pin_schema)"
  recorded_pin="$(meta_value "$meta_snapshot" pin)"
  recorded_sha="$(meta_value "$meta_snapshot" sha256)"
  recorded_source="$(meta_value "$meta_snapshot" source)"
  recorded_build_mode="$(meta_value "$meta_snapshot" build_mode)"
  recorded_head="$(meta_value "$meta_snapshot" head)"
  recorded_head_tree="$(meta_value "$meta_snapshot" head_tree)"
  recorded_bound="$(meta_value "$meta_snapshot" commit_bound)"
  recorded_manifest_schema="$(meta_value "$meta_snapshot" manifest_schema)"
  recorded_policy="$(meta_value "$meta_snapshot" policy_sha256)"
  recorded_tree="$(meta_value "$meta_snapshot" src_tree)"
  recorded_count="$(meta_value "$meta_snapshot" src_count)"
  recorded_list="$(meta_value "$meta_snapshot" src_list_sha256)"

  if [[ "$recorded_schema" != "$PIN_SCHEMA" || "$recorded_pin" != "$pin" \
     || ! "$recorded_sha" =~ ^[0-9a-f]{64}$ \
     || ! "$recorded_tree" =~ ^[0-9a-f]{64}$ || ! "$recorded_policy" =~ ^[0-9a-f]{64}$ \
     || ! "$recorded_list" =~ ^[0-9a-f]{64}$ || ! "$recorded_count" =~ ^[1-9][0-9]*$ \
     || ! "$recorded_head" =~ ^[0-9a-f]{40}$ \
     || ! "$recorded_head_tree" =~ ^([0-9a-f]{40}|[0-9a-f]{64})$ \
     || ! "$recorded_bound" =~ ^(true|false)$ ]]; then
    echo "PIN_META_INVALID: malformed or inconsistent versioned metadata" >&2
    return 1
  fi
  if [[ "$recorded_bound" == "true" ]]; then
    if [[ "$recorded_source" != "fresh-exact-head-archive" \
       || "$recorded_build_mode" != "cargo-build-locked-release-exact-head-archive-clean-target" ]]; then
      echo "PIN_META_INVALID: release pin lacks the mandatory exact-HEAD archive build binding" >&2
      return 1
    fi
  elif [[ "$recorded_source" != "$SRC" || "$recorded_build_mode" != "technical-existing-target" ]]; then
    echo "PIN_META_INVALID: technical pin has an invalid binary origin" >&2
    return 1
  fi

  local expected_name
  expected_name="anubis-${actual_sha:0:12}-src-${recorded_tree:0:12}"
  [[ "$recorded_bound" == "true" ]] && expected_name="${expected_name}-release"
  if [[ "$actual_sha" != "$recorded_sha" || "$(basename "$pin")" != "$expected_name" ]]; then
    echo "PIN_BYTES_MISMATCH: pin=$pin actual=$actual_sha metadata=$recorded_sha" >&2
    return 1
  fi

  actual_manifest="$scratch/live-manifest.json"
  pin_manifest --field json > "$actual_manifest"
  actual_summary="$(manifest_summary_from_json "$actual_manifest")"
  local actual_schema actual_policy actual_tree actual_count actual_list
  actual_schema="$(printf '%s\n' "$actual_summary" | awk -F': ' '$1 == "manifest_schema" { print $2 }')"
  actual_policy="$(printf '%s\n' "$actual_summary" | awk -F': ' '$1 == "policy_sha256" { print $2 }')"
  actual_tree="$(printf '%s\n' "$actual_summary" | awk -F': ' '$1 == "src_tree" { print $2 }')"
  actual_count="$(printf '%s\n' "$actual_summary" | awk -F': ' '$1 == "src_count" { print $2 }')"
  actual_list="$(printf '%s\n' "$actual_summary" | awk -F': ' '$1 == "src_list_sha256" { print $2 }')"
  if [[ "$recorded_manifest_schema" != "$actual_schema" || "$recorded_policy" != "$actual_policy" \
     || "$recorded_tree" != "$actual_tree" || "$recorded_count" != "$actual_count" \
     || "$recorded_list" != "$actual_list" ]]; then
    echo "PIN_MANIFEST_MISMATCH: PIN DOES NOT MATCH THE TREE" >&2
    echo "  pin:        $pin" >&2
    echo "  pin src:    schema=$recorded_manifest_schema policy=$recorded_policy tree=$recorded_tree count=$recorded_count list=$recorded_list" >&2
    echo "  actual src: schema=$actual_schema policy=$actual_policy tree=$actual_tree count=$actual_count list=$actual_list" >&2
    return 1
  fi

  if [[ "$require_release" == "1" ]]; then
    if [[ "$recorded_bound" != "true" ]]; then
      echo "PIN_RELEASE_UNBOUND: current pin is a technical pin, not a commit-bound release pin" >&2
      return 1
    fi
    require_clean_commit_binding "$actual_manifest" "$recorded_head" "$recorded_head_tree" "$scratch/release" 1
  fi
  closing_current="$(read_current_pin)" || return 1
  if [[ "$closing_current" != "$pin" ]]; then
    echo "PIN_CURRENT_RACE: CURRENT changed during verification" >&2
    return 1
  fi
  if ! closing_pin_sha="$(stable_snapshot_regular "$pin" "$scratch/pin.closing.snapshot" 1)" \
     || ! closing_meta_sha="$(stable_snapshot_regular "$meta" "$scratch/meta.closing.snapshot" 0)"; then
    echo "PIN_IDENTITY_RACE: pin or metadata could not be closed stably" >&2
    return 1
  fi
  if [[ "$closing_pin_sha" != "$actual_sha" || "$closing_meta_sha" != "$initial_meta_sha" ]]; then
    echo "PIN_IDENTITY_RACE: pin or metadata changed during verification" >&2
    return 1
  fi
  echo "pin matches tree: $pin"
)

case "$MODE" in
  current)
    read_current_pin
    exit 0
    ;;
  verify)
    verify_pin 0
    exit $?
    ;;
  verify-release)
    verify_pin 1
    exit $?
    ;;
esac

if [[ "$MODE" == "publish" ]]; then
  assert_repo_path_components "$SRC" 0
  if [[ ! -f "$SRC" || -L "$SRC" || ! -x "$SRC" ]]; then
    echo "PIN_SOURCE_INVALID: no regular release executable at $SRC; build first" >&2
    exit 1
  fi
fi
assert_repo_path_components "$PIN_DIR" 1
mkdir -p "$PIN_DIR"
assert_repo_path_components "$PIN_DIR" 0

scratch="$(mktemp -d "${TMPDIR:-/tmp}/anubis-pin-publish.XXXXXX")"
scratch="$(cd "$scratch" && pwd -P)"
lock=""
lock_acquired=0
cleanup_publish() {
  if [[ "$lock_acquired" == 1 && -n "$lock" && -d "$lock" && ! -L "$lock" ]]; then
    rmdir "$lock" 2>/dev/null || true
  fi
  rm -rf "$scratch"
}
trap cleanup_publish EXIT

head_full="$("$GIT_BIN" -C "$REPO_ROOT" rev-parse --verify 'HEAD^{commit}' 2>/dev/null)" || {
  echo "PIN_HEAD_INVALID: publication requires a full Git commit identity" >&2
  exit 1
}
head_tree="$("$GIT_BIN" -C "$REPO_ROOT" rev-parse --verify 'HEAD^{tree}' 2>/dev/null)" || {
  echo "PIN_HEAD_INVALID: publication requires a Git tree identity" >&2
  exit 1
}

opening_manifest="$scratch/source-opening.json"
closing_manifest="$scratch/source-closing.json"
final_manifest="$scratch/source-final.json"
pin_manifest --field json > "$opening_manifest"

binary_source="$SRC"
source_label="$SRC"
build_mode="technical-existing-target"
if [[ "$MODE" == "release" ]]; then
  # A prior successful publication necessarily leaves the tracked CURRENT pointer
  # different from HEAD.  Permit that one publication-owned path while still
  # requiring every source-manifest byte to match the exact commit.
  require_clean_commit_binding "$opening_manifest" "$head_full" "$head_tree" "$scratch/release-opening" 1
  if [[ "${ANUBIS_PIN_ALLOW_STALE:-0}" != "0" ]]; then
    echo "PIN_RELEASE_OVERRIDE_DENIED: ANUBIS_PIN_ALLOW_STALE is forbidden for release publication" >&2
    exit 1
  fi
  for weakening_var in ANUBIS_SKIP_RISC0_METAL RISC0_SKIP_BUILD_KERNELS R0_DISABLE_METAL; do
    if [[ "${!weakening_var:-0}" != "0" ]]; then
      echo "PIN_RELEASE_BUILD_ENV_DENIED: $weakening_var must be unset or 0" >&2
      exit 1
    fi
  done
  release_jobs="${ANUBIS_RELEASE_BUILD_JOBS:-3}"
  if [[ ! "$release_jobs" =~ ^[1-6]$ ]]; then
    echo "PIN_RELEASE_BUILD_JOBS_INVALID: expected an integer from 1 to 6" >&2
    exit 2
  fi
  reject_release_build_overrides || exit 1
  if [[ ! -d "$CANONICAL_CARGO_HOME" || -L "$CANONICAL_CARGO_HOME" \
     || ! -d "$CANONICAL_RUSTUP_HOME" || -L "$CANONICAL_RUSTUP_HOME" ]]; then
    echo "PIN_RELEASE_TOOL_UNTRUSTED: canonical Cargo or rustup home is unavailable" >&2
    exit 1
  fi
  if [[ ! -x "$CARGO_BIN" || -d "$CARGO_BIN" ]]; then
    echo "PIN_RELEASE_TOOL_UNTRUSTED: rustup cargo executable is unavailable at $CARGO_BIN" >&2
    exit 1
  fi
  release_source="$scratch/release-opening/archive/tree"
  # Keep the canonical cargo/rustup executables and toolchains, but never load
  # the operator's unbound ~/.cargo/config.toml. The empty per-run Cargo home
  # leaves only exact-HEAD .cargo configuration in Cargo's directory walk.
  release_cargo_home="$scratch/release-cargo-home"
  mkdir "$scratch/build-target" "$release_cargo_home"
  assert_no_parent_cargo_configuration "$release_source" || exit 1
  if ! (cd "$release_source" && \
    "$ENV_BIN" -i \
      HOME="$OPERATOR_HOME" \
      CARGO_HOME="$release_cargo_home" \
      RUSTUP_HOME="$CANONICAL_RUSTUP_HOME" \
      PATH="$CANONICAL_BUILD_PATH" \
      TMPDIR=/tmp \
      CARGO_TARGET_DIR="$scratch/build-target" CARGO_BUILD_JOBS="$release_jobs" \
      "$CARGO_BIN" build --locked --release -p anubis) \
      >"$scratch/release-build.stdout" 2>"$scratch/release-build.stderr"; then
    echo "PIN_RELEASE_BUILD_FAILED: fresh isolated cargo build failed" >&2
    sed -n '1,80p' "$scratch/release-build.stderr" >&2
    exit 1
  fi
  assert_no_parent_cargo_configuration "$release_source" || exit 1
  binary_source="$scratch/build-target/release/anubis"
  source_label="fresh-exact-head-archive"
  build_mode="cargo-build-locked-release-exact-head-archive-clean-target"
  if [[ ! -f "$binary_source" || -L "$binary_source" || ! -x "$binary_source" ]]; then
    echo "PIN_RELEASE_BUILD_FAILED: isolated build did not produce a regular executable" >&2
    exit 1
  fi
elif [[ "${ANUBIS_PIN_ALLOW_STALE:-0}" != "1" ]]; then
  if stale_src="$(pin_manifest --newer-than "$SRC")"; then
    :
  else
    stale_rc=$?
    if [[ "$stale_rc" -ne 3 ]]; then
      echo "REFUSING to publish because the source manifest could not be traversed exactly." >&2
      exit "$stale_rc"
    fi
    echo "REFUSING to publish a stale pin: $stale_src is newer than $SRC" >&2
    exit 1
  fi
fi

snapshot_json="$(snapshot_binary "$binary_source" "$scratch/binary")" || {
  echo "PIN_BINARY_RACE: release binary changed during snapshot" >&2
  exit 1
}
sha="$(printf '%s\n' "$snapshot_json" | trusted_python -c 'import json,sys; print(json.load(sys.stdin)["sha256"])')"

pin_manifest --field json > "$closing_manifest"
if ! cmp -s "$opening_manifest" "$closing_manifest"; then
  echo "PIN_SOURCE_RACE: source manifest changed while the binary was snapshotted" >&2
  exit 1
fi
if [[ "$("$GIT_BIN" -C "$REPO_ROOT" rev-parse --verify 'HEAD^{commit}' 2>/dev/null || true)" != "$head_full" \
   || "$("$GIT_BIN" -C "$REPO_ROOT" rev-parse --verify 'HEAD^{tree}' 2>/dev/null || true)" != "$head_tree" ]]; then
  echo "PIN_HEAD_RACE: Git HEAD changed during publication" >&2
  exit 1
fi
if [[ "$MODE" == "release" ]]; then
  require_clean_commit_binding "$closing_manifest" "$head_full" "$head_tree" "$scratch/release-closing" 1
fi

src_tree="$(json_field "$opening_manifest" tree_sha256)"
short="${sha:0:12}"
src_short="${src_tree:0:12}"
pin="$PIN_DIR/anubis-${short}-src-${src_short}"
[[ "$MODE" == "release" ]] && pin="${pin}-release"
meta="$pin.meta"
# CURRENT is repository-global, so publication must serialize globally rather than
# only against another publisher that happens to derive the same content identity.
lock="$PIN_DIR/.publish.lock"
if ! mkdir "$lock" 2>/dev/null; then
  echo "PIN_PUBLICATION_LOCKED: another or interrupted publisher owns $lock" >&2
  exit 1
fi
lock_acquired=1

manifest_summary="$(manifest_summary_from_json "$opening_manifest")"
commit_bound=false
[[ "$MODE" == "release" ]] && commit_bound=true
{
  echo "pin_schema: $PIN_SCHEMA"
  echo "pin: $pin"
  echo "sha256: $sha"
  echo "source: $source_label"
  echo "build_mode: $build_mode"
  echo "head: $head_full"
  echo "head_tree: $head_tree"
  echo "commit_bound: $commit_bound"
  printf '%s\n' "$manifest_summary"
} > "$scratch/metadata"
chmod 0444 "$scratch/metadata"

# The binary and metadata are two independent no-clobber installs, not an
# atomic pair. Under the global lock, an interrupted binary-first publication
# is recoverable only when that orphan is a stable exact 0555 byte match. A
# malformed, conflicting, or metadata-only orphan remains a collision. The
# completed pair is selected atomically only when CURRENT is replaced below.
pin_present=0
meta_present=0
[[ -e "$pin" || -L "$pin" ]] && pin_present=1
[[ -e "$meta" || -L "$meta" ]] && meta_present=1
recovering_binary_orphan=0

if [[ "$pin_present" == "1" && "$meta_present" == "1" ]]; then
  if ! stable_published_file_equals "$pin" "$scratch/binary" 0555 \
     || ! stable_published_file_equals "$meta" "$scratch/metadata" 0444; then
    echo "PIN_COLLISION: immutable pin identity or metadata already exists with different provenance: $pin" >&2
    exit 1
  fi
elif [[ "$pin_present" == "1" && "$meta_present" == "0" ]]; then
  if ! stable_published_file_equals "$pin" "$scratch/binary" 0555; then
    echo "PIN_COLLISION: malformed or conflicting binary-only orphan: $pin" >&2
    exit 1
  fi
  recovering_binary_orphan=1
elif [[ "$pin_present" == "0" && "$meta_present" == "1" ]]; then
  echo "PIN_COLLISION: metadata-only orphan cannot be recovered: $meta" >&2
  exit 1
else
  if durable_publish_file "$scratch/binary" "$pin" 0555; then
    :
  else
    publish_rc=$?
    echo "PIN_PUBLICATION_FAILED: no-clobber durable pin publication failed (rc=$publish_rc): $pin" >&2
    exit 1
  fi
fi

if [[ "$meta_present" == "0" ]]; then
  if durable_publish_file "$scratch/metadata" "$meta" 0444; then
    :
  else
    publish_rc=$?
    echo "PIN_PUBLICATION_FAILED: no-clobber durable metadata publication failed (rc=$publish_rc): $meta" >&2
    exit 1
  fi
fi
if ! stable_published_file_equals "$pin" "$scratch/binary" 0555 \
   || ! stable_published_file_equals "$meta" "$scratch/metadata" 0444; then
  echo "PIN_PUBLICATION_FAILED: completed immutable pair failed stable validation: $pin" >&2
  exit 1
fi
if [[ "$recovering_binary_orphan" == "1" ]]; then
  echo "PIN_ORPHAN_RECOVERED: completed exact binary/metadata pair: $pin" >&2
fi
rm "$scratch/binary"

# A third complete collection closes edits that race immutable-file installation. A failed closing
# check can leave an unselected exact pair, but retry accepts that pair and CURRENT stays unchanged.
pin_manifest --field json > "$final_manifest"
if ! cmp -s "$opening_manifest" "$final_manifest"; then
  echo "PIN_SOURCE_RACE: source manifest changed before CURRENT publication; CURRENT unchanged" >&2
  exit 1
fi
if [[ "$MODE" == "release" ]]; then
  require_clean_commit_binding "$final_manifest" "$head_full" "$head_tree" "$scratch/release-final" 1
fi

printf '%s\n' "$pin" > "$scratch/CURRENT"
chmod 0644 "$scratch/CURRENT"
assert_repo_path_components "$CURRENT" 1
durable_replace_pointer "$scratch/CURRENT" "$CURRENT"

verify_mode=0
[[ "$MODE" == "release" ]] && verify_mode=1
if ! verify_output="$(verify_pin "$verify_mode")"; then
  echo "PIN_POST_PUBLICATION_VERIFY_FAILED: CURRENT does not verify" >&2
  exit 1
fi
if [[ "$verify_output" != "pin matches tree: $pin" \
   || "$(read_current_pin)" != "$pin" ]]; then
  echo "PIN_POST_PUBLICATION_VERIFY_FAILED: verified identity does not equal the published pin" >&2
  exit 1
fi

printf '%s\n' "$pin"
if [[ "$MODE" == "release" ]]; then
  echo "release pin is commit-bound; verify with: scripts/publish_pin.sh --verify-release" >&2
else
  echo "technical pin only; it is not authorized as a tagged release artifact" >&2
fi
fi
