#!/usr/bin/env bash
# Fail-closed host protection for Anubis Tart/VZ workloads on macOS.
# Safe to source from gate scripts or execute as: host_resource_guard.sh once|watch.

ANUBIS_GUARD_MAX_CPU=8
ANUBIS_GUARD_MAX_MEM_MIB=12288
ANUBIS_HOST_MIN_FREE_MIB=8192
ANUBIS_HOST_START_MIN_FREE_MIB=12288
ANUBIS_GUARD_INTERVAL_SECS=5
ANUBIS_GUARD_TART_BIN="${ANUBIS_GUARD_TART_BIN:-$(command -v tart 2>/dev/null || true)}"
ANUBIS_GUARD_KEEP_DIR="${ANUBIS_GUARD_KEEP_DIR:-$HOME/Library/Application Support/Anubis/kept-vms}"
ANUBIS_GUARD_QUIET_OK="${ANUBIS_GUARD_QUIET_OK:-0}"

anubis_guard_rsync() {
  if [[ ! -f /usr/bin/rsync || -L /usr/bin/rsync || ! -x /usr/bin/rsync ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_TOOL: trusted /usr/bin/rsync is unavailable" >&2
    return 2
  fi
  /usr/bin/rsync "$@"
}

anubis_guard_ssh() {
  if [[ ! -f /usr/bin/ssh || -L /usr/bin/ssh || ! -x /usr/bin/ssh ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_TOOL: trusted /usr/bin/ssh is unavailable" >&2
    return 2
  fi
  /usr/bin/ssh "$@"
}

anubis_guard_mark_kept() {
  [[ $# -eq 1 && "$1" != */* ]] || return 2
  mkdir -p "$ANUBIS_GUARD_KEEP_DIR"
  : >"$ANUBIS_GUARD_KEEP_DIR/$1"
}
anubis_guard_is_kept() {
  [[ $# -eq 1 && "$1" != */* && -f "$ANUBIS_GUARD_KEEP_DIR/$1" ]]
}
anubis_guard_clear_kept() {
  [[ $# -eq 1 && "$1" != */* ]] || return 2
  rm -f "$ANUBIS_GUARD_KEEP_DIR/$1"
}

anubis_guard_validate_vm_limits() {
  if [[ $# -ne 2 ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: expected CPU and memory MiB" >&2
    return 2
  fi
  local cpu="$1" mem="$2"
  # Bound the lexical width before Bash arithmetic.  Otherwise an attacker can
  # feed an arbitrarily long decimal that wraps during `(( ... ))` and evade a
  # ceiling check.  The configured maxima fit in these canonical widths.
  if [[ ! "$cpu" =~ ^[1-9][0-9]{0,4}$ || ! "$mem" =~ ^[1-9][0-9]{0,4}$ ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: CPU and memory must be positive integers (cpu=$cpu mem=$mem)" >&2
    return 2
  fi
  if (( 10#$cpu > ANUBIS_GUARD_MAX_CPU )); then
    echo "ANUBIS_HOST_GUARD_CPU_CEILING: requested=$cpu max=$ANUBIS_GUARD_MAX_CPU" >&2
    return 1
  fi
  if (( 10#$mem > ANUBIS_GUARD_MAX_MEM_MIB )); then
    echo "ANUBIS_HOST_GUARD_MEMORY_CEILING: requested=${mem}MiB max=${ANUBIS_GUARD_MAX_MEM_MIB}MiB" >&2
    return 1
  fi
}

# Copy the live repository into a disposable guest without traversing host-local
# artifact forests. The selected immutable pin is copied; archived pins stay untouched. The guest
# target cache is deliberately preserved, while agent worktrees, exports, and scratch evidence are
# removed explicitly before sync.
# Keeping this seam here makes the full slice and offensive gate use one policy;
# a new launcher cannot silently reintroduce the measured 36-GiB cache blowout.
anubis_guard_selected_pin_identity() {
  if [[ $# -ne 1 || -z "$1" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: selected-pin identity requires a source root" >&2
    return 2
  fi
  /usr/bin/python3 -I -B - "$1" <<'PY'
import hashlib
import os
import re
import stat
import sys
from pathlib import Path

root = Path(sys.argv[1])


def fail(message: str) -> None:
    print(f"ANUBIS_HOST_GUARD_SYNC_PIN: {message}", file=sys.stderr)
    raise SystemExit(2)


def identity(metadata: os.stat_result) -> tuple[int, ...]:
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
        metadata.st_mode,
    )


def require_real_directory(path: Path, label: str) -> None:
    try:
        metadata = os.lstat(path)
    except OSError as exc:
        fail(f"cannot stat {label}: {exc}")
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
        fail(f"{label} must be a real directory: {path}")


def stable_file(path: Path, label: str, *, executable: bool = False) -> tuple[bytes, str]:
    try:
        before = os.lstat(path)
    except OSError as exc:
        fail(f"cannot stat {label}: {exc}")
    if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
        fail(f"{label} must be a regular non-symlink file: {path}")
    if executable and not before.st_mode & 0o111:
        fail(f"{label} must be executable: {path}")
    if label in ("selected pin", "selected pin metadata") and before.st_mode & 0o222:
        fail(f"{label} must be non-writable: {path}")

    flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
    digest = hashlib.sha256()
    chunks: list[bytes] = []
    try:
        descriptor = os.open(path, flags)
        with os.fdopen(descriptor, "rb") as handle:
            opened = os.fstat(handle.fileno())
            if identity(opened) != identity(before):
                fail(f"{label} changed while opening: {path}")
            while True:
                chunk = handle.read(1024 * 1024)
                if not chunk:
                    break
                digest.update(chunk)
                if label != "selected pin":
                    chunks.append(chunk)
            after = os.fstat(handle.fileno())
    except OSError as exc:
        fail(f"cannot read {label}: {exc}")
    try:
        path_after = os.lstat(path)
    except OSError as exc:
        fail(f"cannot restat {label}: {exc}")
    if identity(before) != identity(after) or identity(after) != identity(path_after):
        fail(f"{label} changed while hashing: {path}")
    return b"".join(chunks), digest.hexdigest()


for directory, label in (
    (root, "source root"),
    (root / "vm", "vm directory"),
    (root / "vm" / "pins", "pin directory"),
):
    require_real_directory(directory, label)

current_bytes, current_sha = stable_file(root / "vm" / "pins" / "CURRENT", "CURRENT")
try:
    current_text = current_bytes.decode("ascii")
except UnicodeDecodeError:
    fail("CURRENT is not ASCII")
if not current_text.endswith("\n") or current_text.count("\n") != 1:
    fail("CURRENT must contain exactly one newline-terminated pin reference")
current_ref = current_text[:-1]
match = re.fullmatch(
    r"(?:vm/pins/)?(anubis-[0-9a-f]{12}(?:-src-[0-9a-f]{12}(?:-release)?)?)",
    current_ref,
)
if match is None:
    fail("CURRENT contains an invalid pin reference")
pin_name = match.group(1)
pin_rel = f"vm/pins/{pin_name}"
pin_path = root / pin_rel
meta_path = root / f"{pin_rel}.meta"
_, pin_sha = stable_file(pin_path, "selected pin", executable=True)
meta_bytes, meta_sha = stable_file(meta_path, "selected pin metadata")

try:
    metadata_text = meta_bytes.decode("utf-8")
except UnicodeDecodeError:
    fail("selected pin metadata is not UTF-8")
fields: dict[str, str] = {}
for line in metadata_text.splitlines():
    if not line or ":" not in line:
        fail("selected pin metadata contains a malformed line")
    key, value = line.split(":", 1)
    key = key.strip()
    value = value.strip()
    if not re.fullmatch(r"[a-z][a-z0-9_]*", key) or not value or key in fields:
        fail(f"selected pin metadata contains an invalid or duplicate field: {key!r}")
    fields[key] = value

for required in ("pin", "sha256", "source"):
    if required not in fields:
        fail(f"selected pin metadata is missing {required}")
if fields["pin"] != pin_rel or fields["sha256"] != pin_sha:
    fail("selected pin metadata does not match its path and bytes")
if not re.fullmatch(r"[0-9a-f]{64}", fields["sha256"]):
    fail("selected pin metadata sha256 is malformed")

if "pin_schema" in fields:
    required_v2 = {
        "pin_schema",
        "pin",
        "sha256",
        "source",
        "build_mode",
        "head",
        "head_tree",
        "commit_bound",
        "manifest_schema",
        "policy_sha256",
        "src_tree",
        "src_count",
        "src_list_sha256",
    }
    missing = sorted(required_v2 - fields.keys())
    if missing:
        fail(f"versioned pin metadata is missing fields: {missing}")
    if fields["pin_schema"] != "anubis.binary-pin.v2":
        fail("selected pin metadata has an unsupported schema")
    for key in ("policy_sha256", "src_tree", "src_list_sha256"):
        if not re.fullmatch(r"[0-9a-f]{64}", fields[key]):
            fail(f"versioned pin metadata has malformed {key}")
    if not re.fullmatch(r"[0-9a-f]{40}", fields["head"]):
        fail("versioned pin metadata has malformed head")
    if not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", fields["head_tree"]):
        fail("versioned pin metadata has malformed head_tree")
    if not re.fullmatch(r"[1-9][0-9]*", fields["src_count"]):
        fail("versioned pin metadata has malformed src_count")
    if fields["manifest_schema"] != "anubis.pin-source-manifest.v2":
        fail("versioned pin metadata has an unsupported manifest schema")
    if fields["commit_bound"] == "true":
        expected_name = f"anubis-{pin_sha[:12]}-src-{fields['src_tree'][:12]}-release"
        if fields["source"] != "fresh-exact-head-archive" or fields["build_mode"] != "cargo-build-locked-release-exact-head-archive-clean-target":
            fail("release pin metadata lacks the exact-HEAD archive build binding")
    elif fields["commit_bound"] == "false":
        expected_name = f"anubis-{pin_sha[:12]}-src-{fields['src_tree'][:12]}"
        if fields["source"] != "target/release/anubis" or fields["build_mode"] != "technical-existing-target":
            fail("technical pin metadata has an invalid binary origin")
    else:
        fail("versioned pin metadata has malformed commit_bound")
else:
    expected_name = f"anubis-{pin_sha[:12]}"
    if fields["source"] != "target/release/anubis":
        fail("legacy pin metadata has an invalid binary origin")

if pin_name != expected_name:
    fail("selected pin filename is inconsistent with metadata and bytes")

print(f"current_sha256={current_sha}")
print(f"pin={pin_rel}")
print(f"pin_sha256={pin_sha}")
print(f"meta_sha256={meta_sha}")
PY
}

# Send the already-parsed stable-receipt implementation over SSH instead of
# trusting a guest-side helper path that could itself be substituted. The
# remote invocation uses Apple's absolute Python path and returns the same
# four-line receipt as the host implementation.
anubis_guard_remote_selected_pin_identity() {
  if [[ $# -ne 3 || -z "$1" || -z "$2" || -z "$3" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: remote identity requires shell, host, and path" >&2
    return 2
  fi
  local rsh="$1" remote_host="$2" remote_rel="$3"
  local -a remote_argv remote_options
  read -r -a remote_argv <<<"$rsh"
  (( ${#remote_argv[@]} > 0 )) || return 2
  case "${remote_argv[0]}" in
    ssh|/usr/bin/ssh) ;;
    *)
      echo "ANUBIS_HOST_GUARD_SYNC_TOOL: remote shell must be trusted /usr/bin/ssh" >&2
      return 2
      ;;
  esac
  remote_options=("${remote_argv[@]:1}")
  {
    declare -f anubis_guard_selected_pin_identity
    printf '\nanubis_guard_selected_pin_identity "$1"\n'
  } | anubis_guard_ssh "${remote_options[@]}" "$remote_host" \
    "/bin/bash -s -- \"\$HOME/$remote_rel\" # ANUBIS_GUEST_PIN_IDENTITY_V1"
}

anubis_guard_sync_tree() {
  if [[ $# -ne 3 || -z "$1" || -z "$2" || -z "$3" ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: sync_tree requires RSYNC_RSH, source, and destination" >&2
    return 2
  fi
  local rsh="$1" source="$2" destination="$3" source_root remote_host remote_path remote_rel
  local current_pin_ref current_pin_name current_pin_path current_meta manifest
  local pin_identity_before pin_identity_after guest_pin_identity guest_pin_identity_after
  local -a rsh_argv ssh_options
  source_root="${source%/}"
  if [[ "$source_root" != /* ]]; then source_root="$PWD/$source_root"; fi
  local source_component="$source_root"
  while [[ "$source_component" != / ]]; do
    if [[ -L "$source_component" ]]; then
      echo "ANUBIS_HOST_GUARD_SYNC_SOURCE: symlinked source component: $source_component" >&2
      return 2
    fi
    source_component="${source_component%/*}"
    [[ -n "$source_component" ]] || source_component=/
  done
  if [[ ! -d "$source_root" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_SOURCE: source must be a real directory: $source" >&2
    return 2
  fi
  local pin_dir
  for pin_dir in "$source_root/vm" "$source_root/vm/pins"; do
    if [[ ! -d "$pin_dir" || -L "$pin_dir" ]]; then
      echo "ANUBIS_HOST_GUARD_SYNC_PIN: pin path ancestor is missing or symlinked: $pin_dir" >&2
      return 2
    fi
  done
  if ! pin_identity_before="$(anubis_guard_selected_pin_identity "$source_root")"; then
    return 2
  fi
  current_pin_ref="$(printf '%s\n' "$pin_identity_before" | awk -F= '$1 == "pin" { print $2 }')"
  [[ -n "$current_pin_ref" ]] || {
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: validated identity omitted the pin path" >&2
    return 2
  }
  current_pin_name="${current_pin_ref##*/}"
  current_pin_path="$source_root/vm/pins/$current_pin_name"
  current_meta="$current_pin_path.meta"
  for manifest in "$current_pin_path" "$current_meta"; do
    if [[ ! -f "$manifest" || -L "$manifest" ]]; then
      echo "ANUBIS_HOST_GUARD_SYNC_PIN: selected pin component missing/non-regular/symlinked: $manifest" >&2
      return 2
    fi
  done
  if [[ "$destination" != *:* ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: expected host:path destination" >&2
    return 2
  fi
  remote_host="${destination%%:*}"
  remote_path="${destination#*:}"
  if [[ ! "$remote_host" =~ ^([A-Za-z0-9_][A-Za-z0-9._-]*@)?[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: unsafe remote host" >&2
    return 2
  fi
  case "$remote_path" in
    '~/'*) remote_rel="${remote_path#\~/}" ;;
    /*) echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: absolute remote path denied" >&2; return 2 ;;
    *) remote_rel="$remote_path" ;;
  esac
  remote_rel="${remote_rel%/}"
  if [[ -z "$remote_rel" || ! "$remote_rel" =~ ^[A-Za-z0-9._/-]+$ \
    || "$remote_rel" == . || "$remote_rel" == .. \
    || "$remote_rel" == ./* || "$remote_rel" == ../* \
    || "$remote_rel" == */./* || "$remote_rel" == */../* \
    || "$remote_rel" == */. || "$remote_rel" == */.. \
    || "$remote_rel" == *//* || "$remote_path" == *//* ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: unsafe remote path" >&2
    return 2
  fi
  read -r -a rsh_argv <<<"$rsh"
  if (( ${#rsh_argv[@]} == 0 )); then
    echo "ANUBIS_HOST_GUARD_SYNC_SHELL: empty remote shell" >&2
    return 2
  fi
  case "${rsh_argv[0]}" in
    ssh|/usr/bin/ssh) ;;
    *)
      echo "ANUBIS_HOST_GUARD_SYNC_SHELL: only trusted /usr/bin/ssh is permitted" >&2
      return 2
      ;;
  esac
  ssh_options=("${rsh_argv[@]:1}")
  if [[ "${rsh_argv[0]}" == ssh ]]; then
    rsh="/usr/bin/ssh${rsh#ssh}"
  fi
  if ! anubis_guard_ssh "${ssh_options[@]}" "$remote_host" \
    "set -u; test ! -L \"\$HOME\" || exit 42; d=\"\$HOME\"; rel=\"$remote_rel\"; while test -n \"\$rel\"; do case \"\$rel\" in */*) part=\"\${rel%%/*}\"; rel=\"\${rel#*/}\" ;; *) part=\"\$rel\"; rel= ;; esac; test -n \"\$part\" || exit 42; d=\"\$d/\$part\"; test -d \"\$d\" || exit 42; test ! -L \"\$d\" || exit 42; done"; then
    echo "ANUBIS_HOST_GUARD_SYNC_DESTINATION: remote tree is missing or symlinked" >&2
    return 2
  fi
  if ! anubis_guard_ssh "${ssh_options[@]}" "$remote_host" \
    "set -u; root=\"\$HOME/$remote_rel\"; git=\"\$root/.git\"; target=\"\$root/target\"; if test -e \"\$git\" || test -L \"\$git\"; then test ! -L \"\$git\" && { test -f \"\$git\" || test -d \"\$git\"; } || exit 42; fi; if test -e \"\$target\" || test -L \"\$target\"; then test -d \"\$target\" && test ! -L \"\$target\" || exit 42; fi"; then
    echo "ANUBIS_HOST_GUARD_SYNC_STATE: preserved .git or target path is unsafe" >&2
    return 2
  fi
  # Sanitize only the pin archive here. All other exclusions come from the
  # versioned source-manifest policy below. The selected immutable pair is
  # synchronized through a narrow rsync allowlist; unrelated malformed archive
  # entries are quarantined without traversing them.
  if ! anubis_guard_ssh "${ssh_options[@]}" "$remote_host" \
    "set -u; root=\"\$HOME/$remote_rel\"; q=\$(mktemp -d \"\$HOME/.anubis-sync-quarantine.XXXXXX\") || exit 42; if test -e \"\$root/vm\" || test -L \"\$root/vm\"; then test -d \"\$root/vm\" && test ! -L \"\$root/vm\" || exit 42; else mkdir \"\$root/vm\" || exit 42; fi; if test -e \"\$root/vm/pins\" || test -L \"\$root/vm/pins\"; then test -d \"\$root/vm/pins\" && test ! -L \"\$root/vm/pins\" || exit 42; else mkdir \"\$root/vm/pins\" || exit 42; fi; setopt NULL_GLOB 2>/dev/null || :; for p in \"\$root/vm/pins/\"* \"\$root/vm/pins/\".[!.]* \"\$root/vm/pins/\"..?*; do if ! test -e \"\$p\" && ! test -L \"\$p\"; then continue; fi; base=\${p##*/}; is_meta=0; candidate=\$base; case \"\$base\" in *.meta) is_meta=1; candidate=\${base%.meta} ;; esac; case \"\$candidate\" in anubis-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]|anubis-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]-src-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]|anubis-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]-src-[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]-release) if test -f \"\$p\" && test ! -L \"\$p\" && { test \"\$is_meta\" = 0 || test ! -w \"\$p\"; }; then continue; fi ;; esac; mv \"\$p\" \"\$q/pin__\$base\" || exit 42; done"; then
    echo "ANUBIS_HOST_GUARD_SYNC_CLEANUP: could not sanitize the guest pin archive" >&2
    return 2
  fi
  local manifest_tool="$source/scripts/lib/pin_manifest.py"
  local manifest_policy="$source/scripts/lib/pin_manifest_policy.json"
  if [[ ! -f "$manifest_tool" || -L "$manifest_tool" \
     || ! -f "$manifest_policy" || -L "$manifest_policy" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_POLICY: source manifest tool/policy missing or unsafe" >&2
    return 2
  fi
  local policy_exclude_output
  if ! policy_exclude_output="$(/usr/bin/python3 -I -B "$manifest_tool" \
    --root "${source%/}" \
    --policy scripts/lib/pin_manifest_policy.json \
    --print-rsync-excludes)"; then
    echo "ANUBIS_HOST_GUARD_SYNC_POLICY: could not derive exact generated-directory filters" >&2
    return 2
  fi
  local -a policy_exclude_args=()
  local policy_exclude policy_pin_archive_exclusion=0 policy_git_exclusion=0
  local policy_target_exclusion=0
  while IFS= read -r policy_exclude; do
    [[ -n "$policy_exclude" ]] || continue
    policy_rel="${policy_exclude#/}"
    policy_rel="${policy_rel%/}"
    if [[ -z "$policy_rel" || ! "$policy_rel" =~ ^[A-Za-z0-9._/-]+$ \
      || "$policy_rel" == . || "$policy_rel" == .. \
      || "$policy_rel" == ./* || "$policy_rel" == ../* \
      || "$policy_rel" == */./* || "$policy_rel" == */../* \
      || "$policy_rel" == */. || "$policy_rel" == */.. \
      || "$policy_rel" == *//* ]]; then
      echo "ANUBIS_HOST_GUARD_SYNC_POLICY: unsafe exported exclusion: $policy_exclude" >&2
      return 2
    fi
    case "$policy_exclude" in
      /vm/pins/)
      # vm/pins is excluded from the source manifest, but sync has a narrower
      # allowlist below for CURRENT plus its selected immutable pair.  A broad
      # rsync exclusion here would shadow those later includes and deleting or
      # quarantining the whole directory would discard safe archived binaries.
        policy_pin_archive_exclusion=$((policy_pin_archive_exclusion + 1))
        continue
        ;;
      /.git) policy_git_exclusion=$((policy_git_exclusion + 1)) ;;
      /target/) policy_target_exclusion=$((policy_target_exclusion + 1)) ;;
    esac
    policy_exclude_args+=(--exclude="$policy_exclude")
  done <<<"$policy_exclude_output"
  if [[ ${#policy_exclude_args[@]} -eq 0 || "$policy_pin_archive_exclusion" -ne 1 \
     || "$policy_git_exclusion" -ne 1 || "$policy_target_exclusion" -ne 1 ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_POLICY: exclusion roster lacks exact .git, target, or vm/pins ownership" >&2
    return 2
  fi

  # Excluding a path from rsync alone preserves a stale copy already present in
  # the golden guest. Quarantine every policy-excluded path except the explicit
  # guest-state exceptions: .git, the warm target cache, and the selected pin
  # archive. Validate every internal ancestor without following symlinks before
  # moving a path; a poisoned guest must never redirect quarantine outside root.
  # Preserve target/ as the sole excluded guest cache; .git is repository state,
  # while vm/pins is constrained separately to CURRENT and its immutable pair.
  # The policy-owned top-level implementer exclusion also removes the historical
  # implementer/a_plus_audit_run/ receipt path instead of syncing stale evidence.
  local policy_quarantine_specs="" policy_rel policy_index=0
  while IFS= read -r policy_exclude; do
    [[ -n "$policy_exclude" ]] || continue
    case "$policy_exclude" in
      /.git|/target/|/vm/pins/) continue ;;
    esac
    policy_rel="${policy_exclude#/}"
    policy_rel="${policy_rel%/}"
    if [[ -z "$policy_rel" || ! "$policy_rel" =~ ^[A-Za-z0-9._/-]+$ \
      || "$policy_rel" == . || "$policy_rel" == .. \
      || "$policy_rel" == ./* || "$policy_rel" == ../* \
      || "$policy_rel" == */./* || "$policy_rel" == */../* \
      || "$policy_rel" == */. || "$policy_rel" == */.. \
      || "$policy_rel" == *//* ]]; then
      echo "ANUBIS_HOST_GUARD_SYNC_POLICY: unsafe exported exclusion: $policy_exclude" >&2
      return 2
    fi
    policy_quarantine_specs="$policy_quarantine_specs $policy_rel=policy__$policy_index"
    policy_index=$((policy_index + 1))
  done <<<"$policy_exclude_output"
  if [[ "$policy_index" -eq 0 ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_POLICY: no policy-owned stale paths are quarantined" >&2
    return 2
  fi
  if ! anubis_guard_ssh "${ssh_options[@]}" "$remote_host" \
    "set -u; root=\"\$HOME/$remote_rel\"; q=\$(mktemp -d \"\$HOME/.anubis-policy-quarantine.XXXXXX\") || exit 42; for spec in$policy_quarantine_specs; do rel=\${spec%%=*}; name=\${spec#*=}; parent=\$root; rest=\$rel; missing=0; while test \"\${rest#*/}\" != \"\$rest\"; do part=\${rest%%/*}; rest=\${rest#*/}; parent=\"\$parent/\$part\"; if test -L \"\$parent\"; then exit 42; elif test -e \"\$parent\"; then test -d \"\$parent\" || exit 42; else missing=1; break; fi; done; test \"\$missing\" = 0 || continue; src=\"\$root/\$rel\"; if test -e \"\$src\" || test -L \"\$src\"; then mv \"\$src\" \"\$q/\$name\" || exit 42; fi; done"; then
    echo "ANUBIS_HOST_GUARD_SYNC_POLICY: could not quarantine stale generated guest paths" >&2
    return 2
  fi

  if ! RSYNC_RSH="$rsh" anubis_guard_rsync -aH --checksum --no-times --delete --no-devices --no-specials \
      "${policy_exclude_args[@]}" \
      --include=/vm/pins/ \
      --include=/vm/pins/CURRENT \
      --include="/vm/pins/$current_pin_name" \
      --include="/vm/pins/$current_pin_name.meta" \
      --exclude='/vm/pins/*' \
      -- "$source" "$destination"; then
    echo "ANUBIS_HOST_GUARD_SYNC_FAILED: rsync did not complete" >&2
    return 2
  fi
  if ! guest_pin_identity="$(anubis_guard_remote_selected_pin_identity \
    "$rsh" "$remote_host" "$remote_rel" | /usr/bin/sed '1{/^ANUBIS_GUEST_PIN_IDENTITY_V1$/d;}')"; then
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: could not take a stable selected-pin receipt in the guest" >&2
    return 2
  fi
  if [[ "$guest_pin_identity" != "$pin_identity_before" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: guest selected pin bytes do not match the stable host identity" >&2
    return 2
  fi
  if ! pin_identity_after="$(anubis_guard_selected_pin_identity "$source_root")"; then
    return 2
  fi
  if [[ "$pin_identity_after" != "$pin_identity_before" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: selected pin or metadata changed during sync" >&2
    return 2
  fi
  if ! guest_pin_identity_after="$(anubis_guard_remote_selected_pin_identity \
    "$rsh" "$remote_host" "$remote_rel" | /usr/bin/sed '1{/^ANUBIS_GUEST_PIN_IDENTITY_V1$/d;}')"; then
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: could not close the guest selected-pin receipt" >&2
    return 2
  fi
  if [[ "$guest_pin_identity_after" != "$guest_pin_identity" \
     || "$guest_pin_identity_after" != "$pin_identity_before" ]]; then
    echo "ANUBIS_HOST_GUARD_SYNC_PIN: guest selected pin, metadata, or CURRENT changed after sync" >&2
    return 2
  fi
}

# Parse vm_stat from stdin and report immediately unused memory. Inactive/cache
# pages are deliberately excluded: the prior WindowServer watchdog event occurred
# with memoryPressure=false, so pressure/reclaimability alone is not a safe signal.
anubis_guard_free_mib_from_vm_stat() {
  awk '
    /page size of [0-9]+ bytes/ {
      line=$0; sub(/^.*page size of /, "", line); sub(/ bytes.*$/, "", line); page=line+0
    }
    /^Pages free:/ { value=$3; gsub(/\./, "", value); free=value+0 }
    /^Pages speculative:/ { value=$3; gsub(/\./, "", value); speculative=value+0 }
    END {
      if (page <= 0) exit 2
      printf "%d\n", int(((free + speculative) * page) / 1048576)
    }
  '
}

anubis_guard_generated_owner_pid() {
  if [[ $# -ne 1 ]]; then return 2; fi
  if [[ "$1" =~ ^(anubis-run|anubis-offensive-gate|anubis-poc-kit-gate|anubis-vz-ephemeral)-([1-9][0-9]*)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[2]}"
    return 0
  fi
  return 1
}

anubis_guard_read_vm_stat() { /usr/bin/vm_stat; }
anubis_guard_read_pressure() { /usr/sbin/sysctl -n kern.memorystatus_vm_pressure_level; }
anubis_guard_read_tart_json() {
  [[ -n "$ANUBIS_GUARD_TART_BIN" ]] || return 1
  "$ANUBIS_GUARD_TART_BIN" list --source local --format json
}
anubis_guard_tart_stop() {
  "$ANUBIS_GUARD_TART_BIN" stop "$1" --timeout 5 >/dev/null 2>&1
}
anubis_guard_tart_delete() {
  "$ANUBIS_GUARD_TART_BIN" delete "$1" >/dev/null 2>&1
}
anubis_guard_owner_alive() { kill -0 "$1" >/dev/null 2>&1; }

anubis_guard_json_rows() {
  /usr/bin/python3 -I -B -c '
import json, sys
for vm in json.load(sys.stdin):
    name = vm.get("Name")
    if isinstance(name, str):
        key = chr(82)+chr(117)+chr(110)+chr(110)+chr(105)+chr(110)+chr(103)
        value = vm.get(key)
        if type(value) is not bool:
            raise SystemExit(f"invalid Tart inventory type for {name}")
        print(f"{name}\t{1 if value else 0}")
'
}

anubis_guard_preflight() {
  if [[ $# -ne 2 ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: preflight requires CPU and memory MiB" >&2
    return 2
  fi
  anubis_guard_validate_vm_limits "$1" "$2" || return
  local pressure free_mib required_mib
  pressure="$(anubis_guard_read_pressure 2>/dev/null)" || {
    echo "ANUBIS_HOST_GUARD_UNREADABLE: cannot read macOS memory pressure" >&2
    return 1
  }
  free_mib="$(anubis_guard_read_vm_stat | anubis_guard_free_mib_from_vm_stat)" || {
    echo "ANUBIS_HOST_GUARD_UNREADABLE: cannot calculate immediately free memory" >&2
    return 1
  }
  if [[ "$pressure" != 1 ]]; then
    echo "ANUBIS_HOST_GUARD_PRESSURE: level=$pressure; refusing to start VM" >&2
    return 1
  fi
  required_mib=$(( $2 + ANUBIS_HOST_MIN_FREE_MIB ))
  if (( required_mib < ANUBIS_HOST_START_MIN_FREE_MIB )); then
    required_mib=$ANUBIS_HOST_START_MIN_FREE_MIB
  fi
  if (( free_mib < required_mib )); then
    echo "ANUBIS_HOST_GUARD_HEADROOM: free=${free_mib}MiB required=${required_mib}MiB (guest=$2MiB + host_reserve=${ANUBIS_HOST_MIN_FREE_MIB}MiB); refusing to start VM" >&2
    return 1
  fi
  printf 'ANUBIS_HOST_GUARD_PREFLIGHT: PASS cpu=%s mem=%sMiB free=%sMiB required=%sMiB pressure=%s\n' "$1" "$2" "$free_mib" "$required_mib" "$pressure"
}

anubis_guard_launchctl_print() {
  launchctl print "gui/${UID}/com.anubis.host-resource-guard"
}

anubis_guard_require_launchagent() {
  local status
  status="$(anubis_guard_launchctl_print 2>/dev/null)" || {
    echo "ANUBIS_HOST_GUARD_LAUNCHAGENT_MISSING: com.anubis.host-resource-guard is not loaded" >&2
    return 1
  }
  if ! grep -Eq 'state[[:space:]]*=[[:space:]]*running' <<<"$status"; then
    echo "ANUBIS_HOST_GUARD_LAUNCHAGENT_INACTIVE: com.anubis.host-resource-guard is not running" >&2
    return 1
  fi
  printf 'ANUBIS_HOST_GUARD_LAUNCHAGENT: PASS\n'
}

anubis_guard_guest_absent() {
  if [[ $# -ne 1 || -z "$1" ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: guest_absent requires a guest name" >&2
    return 2
  fi
  local json rows name running
  json="$(anubis_guard_read_tart_json 2>/dev/null)" || {
    echo "ANUBIS_HOST_GUARD_UNAVAILABLE: cannot verify guest teardown" >&2
    return 1
  }
  rows="$(printf '%s\n' "$json" | anubis_guard_json_rows)" || return 1
  while IFS=$'\t' read -r name running; do
    if [[ "$name" == "$1" ]]; then
      echo "ANUBIS_HOST_GUARD_GUEST_SURVIVED: $1" >&2
      return 1
    fi
  done <<<"$rows"
}

anubis_guard_guest_running() {
  if [[ $# -ne 1 || -z "$1" ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: guest_running requires a guest name" >&2
    return 2
  fi
  local json rows name running
  json="$(anubis_guard_read_tart_json 2>/dev/null)" || return 1
  rows="$(printf '%s\n' "$json" | anubis_guard_json_rows)" || return 1
  while IFS=$'\t' read -r name running; do
    if [[ "$name" == "$1" && "$running" == 1 ]]; then
      return 0
    fi
  done <<<"$rows"
  return 1
}

# Stop/delete a generated guest and grade the final observable state, not the stop command's
# intermediate status. The runtime breaker may have already stopped the VM; that is safe when
# deletion succeeds and the inventory proves the guest absent.
anubis_guard_teardown_guest() {
  if [[ $# -ne 1 || -z "$1" ]]; then
    echo "ANUBIS_HOST_GUARD_INVALID: teardown_guest requires a guest name" >&2
    return 2
  fi
  local guest="$1" stop_rc=0 delete_rc=0
  anubis_guard_tart_stop "$guest" || stop_rc=$?
  anubis_guard_tart_delete "$guest" || delete_rc=$?
  if anubis_guard_guest_absent "$guest"; then
    printf 'ANUBIS_HOST_GUARD_TEARDOWN: PASS guest=%s stop_rc=%s delete_rc=%s\n' \
      "$guest" "$stop_rc" "$delete_rc"
    return 0
  fi
  echo "ANUBIS_HOST_GUARD_TEARDOWN: FAIL guest=$guest stop_rc=$stop_rc delete_rc=$delete_rc" >&2
  return 1
}

anubis_guard_require_torn_down() {
  if [[ "${1:-}" == "torn_down" ]]; then
    return 0
  fi
  echo "ANUBIS_HOST_GUARD_TEARDOWN: final status is not torn_down: ${1:-missing}" >&2
  return 1
}

anubis_guard_start_caffeinate() {
  local parent_pid="${1:-$$}"
  /usr/bin/caffeinate -dimsu -w "$parent_pid" >/dev/null 2>&1 &
  ANUBIS_GUARD_CAFFEINATE_PID=$!
  export ANUBIS_GUARD_CAFFEINATE_PID
}

anubis_guard_watch_once() {
  local pressure free_mib json rows emergency=0 action_failures=0 name running owner
  pressure="$(anubis_guard_read_pressure 2>/dev/null)" || pressure=unknown
  free_mib="$(anubis_guard_read_vm_stat | anubis_guard_free_mib_from_vm_stat 2>/dev/null)" || free_mib=0
  json="$(anubis_guard_read_tart_json 2>/dev/null)" || {
    printf 'ANUBIS_HOST_GUARD_UNAVAILABLE: tart inventory unreadable free=%sMiB pressure=%s\n' "$free_mib" "$pressure" >&2
    return 1
  }
  rows="$(printf '%s\n' "$json" | anubis_guard_json_rows)" || {
    echo "ANUBIS_HOST_GUARD: invalid tart JSON; no destructive action taken" >&2
    return 1
  }

  if [[ "$pressure" != 1 ]] || (( free_mib < ANUBIS_HOST_MIN_FREE_MIB )); then
    emergency=1
    while IFS=$'\t' read -r name running; do
      [[ -n "$name" && "$running" == 1 ]] || continue
      printf 'ANUBIS_HOST_GUARD_EMERGENCY: stopping VM=%s free=%sMiB pressure=%s threshold=%sMiB\n' \
        "$name" "$free_mib" "$pressure" "$ANUBIS_HOST_MIN_FREE_MIB"
      if ! anubis_guard_tart_stop "$name"; then
        printf 'ANUBIS_HOST_GUARD_STOP_FAILED: VM=%s\n' "$name" >&2
        action_failures=$((action_failures + 1))
      fi
    done <<<"$rows"
  fi

  # Generated clones encode their creator PID. If that owner vanished, the clone
  # is an orphan. Named/base/snapshot VMs never match and are never auto-deleted.
  while IFS=$'\t' read -r name running; do
    [[ -n "$name" ]] || continue
    owner="$(anubis_guard_generated_owner_pid "$name" 2>/dev/null)" || continue
    anubis_guard_is_kept "$name" && continue
    anubis_guard_owner_alive "$owner" && continue
    if [[ "$running" == 1 && "$emergency" == 0 ]]; then
      printf 'ANUBIS_HOST_GUARD_ORPHAN: stopping VM=%s owner_pid=%s\n' "$name" "$owner"
      anubis_guard_tart_stop "$name" || {
        printf 'ANUBIS_HOST_GUARD_STOP_FAILED: orphan VM=%s\n' "$name" >&2
        action_failures=$((action_failures + 1))
        continue
      }
    fi
    printf 'ANUBIS_HOST_GUARD_ORPHAN: deleting VM=%s owner_pid=%s\n' "$name" "$owner"
    if ! anubis_guard_tart_delete "$name"; then
      printf 'ANUBIS_HOST_GUARD_DELETE_FAILED: VM=%s\n' "$name" >&2
      action_failures=$((action_failures + 1))
    fi
  done <<<"$rows"

  if [[ "$emergency" != 0 || "$action_failures" != 0 ]]; then
    return 1
  fi

  if [[ "$emergency" == 0 && "$ANUBIS_GUARD_QUIET_OK" != 1 ]]; then
    printf 'ANUBIS_HOST_GUARD_OK: free=%sMiB pressure=%s\n' "$free_mib" "$pressure"
  fi
}

anubis_guard_runtime_watch_loop() {
  local parent_pid="$1" expected_guest="${2:-}"
  while kill -0 "$parent_pid" 2>/dev/null; do
    sleep "$ANUBIS_GUARD_INTERVAL_SECS"
    if ! anubis_guard_watch_once; then
      echo "ANUBIS_HOST_GUARD_RUNTIME_TRIPPED: terminating owner_pid=$parent_pid" >&2
      kill -TERM "$parent_pid" 2>/dev/null || true
      return 1
    fi
    if [[ -n "$expected_guest" ]] && ! anubis_guard_guest_running "$expected_guest"; then
      echo "ANUBIS_HOST_GUARD_GUEST_STOPPED: guest=$expected_guest owner_pid=$parent_pid" >&2
      kill -TERM "$parent_pid" 2>/dev/null || true
      return 1
    fi
  done
}

anubis_guard_start_runtime_watch() {
  local parent_pid="${1:-$$}" expected_guest="${2:-}"
  if [[ ! "$parent_pid" =~ ^[1-9][0-9]*$ ]] || ! kill -0 "$parent_pid" 2>/dev/null; then
    echo "ANUBIS_HOST_GUARD_INVALID: runtime watch requires a live parent PID" >&2
    return 2
  fi
  anubis_guard_watch_once || {
    echo "ANUBIS_HOST_GUARD_RUNTIME_START_FAILED: initial watch check failed" >&2
    return 1
  }
  if [[ -n "$expected_guest" ]] && ! anubis_guard_guest_running "$expected_guest"; then
    echo "ANUBIS_HOST_GUARD_GUEST_NOT_RUNNING: guest=$expected_guest" >&2
    return 1
  fi
  anubis_guard_runtime_watch_loop "$parent_pid" "$expected_guest" &
  ANUBIS_GUARD_RUNTIME_WATCH_PID=$!
  export ANUBIS_GUARD_RUNTIME_WATCH_PID
  kill -0 "$ANUBIS_GUARD_RUNTIME_WATCH_PID" 2>/dev/null || {
    echo "ANUBIS_HOST_GUARD_RUNTIME_START_FAILED: watcher exited during startup" >&2
    return 1
  }
}

anubis_guard_stop_runtime_watch() {
  local watch_pid="${ANUBIS_GUARD_RUNTIME_WATCH_PID:-}"
  [[ -n "$watch_pid" ]] || return 0
  kill "$watch_pid" 2>/dev/null || true
  wait "$watch_pid" 2>/dev/null || true
  ANUBIS_GUARD_RUNTIME_WATCH_PID=""
  export ANUBIS_GUARD_RUNTIME_WATCH_PID
}

anubis_guard_watch() {
  printf 'ANUBIS_HOST_GUARD_STARTED: interval=%ss min_free=%sMiB max_cpu=%s max_mem=%sMiB\n' \
    "$ANUBIS_GUARD_INTERVAL_SECS" "$ANUBIS_HOST_MIN_FREE_MIB" "$ANUBIS_GUARD_MAX_CPU" "$ANUBIS_GUARD_MAX_MEM_MIB"
  while :; do
    anubis_guard_watch_once || true
    sleep "$ANUBIS_GUARD_INTERVAL_SECS"
  done
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  set -u
  case "${1:-once}" in
    once) anubis_guard_watch_once ;;
    watch) anubis_guard_watch ;;
    preflight) anubis_guard_preflight "${2:-}" "${3:-}" ;;
    *) echo "usage: $0 [once|watch|preflight CPU MEM_MIB]" >&2; exit 2 ;;
  esac
fi
