# Shared provenance / report helpers for gate scripts (shell-only; no workloads).
# shellcheck shell=bash

gate_git_head() {
  git -C "${GATE_EVIDENCE_ROOT:-.}" rev-parse HEAD 2>/dev/null || echo ""
}

gate_git_tree() {
  git -C "${GATE_EVIDENCE_ROOT:-.}" rev-parse 'HEAD^{tree}' 2>/dev/null || echo ""
}

gate_git_dirty() {
  # non-empty porcelain => dirty
  if [[ -n "$(git -C "${GATE_EVIDENCE_ROOT:-.}" status --porcelain 2>/dev/null || true)" ]]; then
    echo "true"
  else
    echo "false"
  fi
}

gate_sha256_file() {
  local f="${1:-}"
  if [[ -n "$f" && -f "$f" ]]; then
    shasum -a 256 "$f" 2>/dev/null | awk '{print $1}'
  else
    echo ""
  fi
}

# Merge provenance fields into an existing report.json (or create minimal).
# Args: report_path [binary_path] [teardown_status] [isolation_mode]
gate_augment_report_json() {
  local report_path="$1"
  local binary_path="${2:-}"
  local teardown="${3:-}"
  local isolation_mode="${4:-}"
  local root="${GATE_EVIDENCE_ROOT:-.}"
  python3 - "$report_path" "$binary_path" "$teardown" "$isolation_mode" "$root" <<'PY'
import json, sys, subprocess, os
report_path, binary_path, teardown, isolation_mode, root = sys.argv[1:6]

def git(*args):
    try:
        return subprocess.check_output(["git", "-C", root, *args], text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return ""

def sha256(path):
    if not path or not os.path.isfile(path):
        return ""
    try:
        import hashlib
        h = hashlib.sha256()
        with open(path, "rb") as f:
            for chunk in iter(lambda: f.read(1024 * 1024), b""):
                h.update(chunk)
        return h.hexdigest()
    except Exception:
        return ""

data = {}
if os.path.isfile(report_path):
    try:
        with open(report_path) as f:
            data = json.load(f)
    except Exception:
        data = {}

dirty = bool(git("status", "--porcelain"))
data["git_head"] = git("rev-parse", "HEAD")
data["git_tree"] = git("rev-parse", "HEAD^{tree}")
data["git_dirty"] = dirty
if binary_path:
    data["binary_path"] = binary_path
    data["binary_sha256"] = sha256(binary_path)
if teardown != "":
    data["teardown_status"] = teardown
if isolation_mode:
    data["isolation"] = isolation_mode
# preserve mode alias for consumers
if isolation_mode and "mode" not in data:
    data["mode"] = isolation_mode

with open(report_path, "w") as f:
    json.dump(data, f, indent=2)
    f.write("\n")
print(json.dumps(data, indent=2))
PY
}

# Validate offensive report acceptance.
# Args: report_path expected_mode expected_total
# expected_mode: tart-disposable-guest | host-isolation-witness
gate_validate_offensive_report() {
  local report_path="$1"
  local expected_mode="$2"
  local expected_total="$3"
  python3 - "$report_path" "$expected_mode" "$expected_total" <<'PY'
import json, sys
path, mode, total_s = sys.argv[1:4]
total = int(total_s)
try:
    d = json.load(open(path))
except Exception as e:
    print(f"ANUBIS_G14_REPORT_INVALID: cannot load {path}: {e}", file=sys.stderr)
    sys.exit(1)
iso = d.get("isolation") or d.get("mode") or ""
passed = int(d.get("passed", -1))
failed = int(d.get("failed", -1))
tot = int(d.get("total", -1))
verdict = d.get("overall_verdict", "")
ok = (
    iso == mode
    and passed == total
    and failed == 0
    and tot == total
    and verdict == "PASS"
)
if not ok:
    print(
        f"ANUBIS_G14_REPORT_REJECT: isolation={iso!r} want={mode!r} "
        f"passed={passed} failed={failed} total={tot} want_total={total} verdict={verdict!r}",
        file=sys.stderr,
    )
    sys.exit(1)
print(f"G14_REPORT_OK isolation={iso} {passed}/{tot}")
sys.exit(0)
PY
}
