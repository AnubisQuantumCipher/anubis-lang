#!/usr/bin/env bash
set -euo pipefail
# Run the unified gate suite against a COMMIT, in a throwaway worktree.
#
#   bash scripts/audit_head.sh [--rev REV] [--out DIR] [-- <audit args...>]
#
# WHY THIS EXISTS
#
# `audit_unified.sh` refuses to grade a dirty tree, because a verdict over a moving tree describes
# a state that was never committed. That refusal is correct and it makes the suite unrunnable while
# anyone is working — which, on a repo with four agents editing concurrently, is always.
#
# Both failures are real and this resolves them instead of choosing one:
#
#   - a full run reported `FAIL G1_fmt` while `cargo fmt --check` was clean before AND after,
#     because a file was saved mid-run. The verdict described a four-second tree.
#   - the suite's own G4 rebuilds `target/release/anubis`, which STRIPS the code signature, so a
#     concurrent VZ boot dies on a missing entitlement that has nothing to do with the VZ lane.
#
# A detached worktree has its own checkout and its own `target/`, so the graded tree cannot move
# and the build cannot collide with the main one. The cost is a full cold build, which is the
# honest price of a reproducible verdict.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

REV="HEAD"
OUT=""
PASSTHRU=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --rev) REV="${2:?--rev requires a revision}"; shift 2 ;;
    --out) OUT="${2:?--out requires a directory}"; shift 2 ;;
    --)    shift; PASSTHRU=("$@"); break ;;
    *)     echo "ANUBIS_AUDIT_HEAD_ARGUMENT: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

SHA="$(git rev-parse --verify "$REV^{commit}" 2>/dev/null)" || {
  echo "ANUBIS_AUDIT_HEAD_REV: '$REV' is not a commit" >&2; exit 2; }
SHORT="$(git rev-parse --short "$SHA")"
[[ -n "$OUT" ]] || OUT="out/audit_head/${SHORT}"
mkdir -p "$OUT"

WT="$(mktemp -d -t anubis_audit_head)"
cleanup() {
  # `git worktree remove` refuses while the build directory is populated; --force is correct here
  # because the worktree is a throwaway created seconds ago and holds no authored work.
  git worktree remove --force "$WT" >/dev/null 2>&1 || true
  rm -rf "$WT" 2>/dev/null || true
}
trap cleanup EXIT

echo "[audit-head] revision : $SHORT ($SHA)"
echo "[audit-head] worktree : $WT"
echo "[audit-head] report   : $OUT"

git worktree add --detach "$WT" "$SHA" >/dev/null

# The worktree is a pristine checkout of a commit, so it is clean BY CONSTRUCTION. The override is
# passed for the untracked artifacts a fresh checkout inherits (pins, out/), never to excuse a
# genuinely modified tree — and `audit_unified.sh` records `tree_state` either way, so this cannot
# masquerade as something it is not.
# `( ... ) ; RC=$?` would ABORT here under `set -e` before RC is ever assigned — the suite exits
# non-zero whenever any gate fails, which is the case this script exists to report. That is the
# identical defect this repo found in `fixture_preflight.sh` hours earlier, where a bare `check`
# under `set -e` killed the harness before it printed its own verdict. Written again, in the tool
# built to grade the tools. A non-zero exit here is DATA, not failure.
RC=0
(
  cd "$WT"
  # NOT `"${PASSTHRU[@]+"${PASSTHRU[@]}"}"` — the outer quotes turn an EMPTY array into one empty
  # argument, which `audit_unified.sh` rejects as `unknown argument ''` before printing a single
  # line. Measured: argc=1, arg=[]. The unquoted form expands to nothing, which is what is meant.
  ANUBIS_AUDIT_ALLOW_DIRTY=1 bash scripts/audit_unified.sh --out "$WT/audit_out" ${PASSTHRU[@]+"${PASSTHRU[@]}"}
) || RC=$?

if [[ -d "$WT/audit_out" ]]; then
  cp -R "$WT/audit_out/." "$OUT/" 2>/dev/null || true
  # Stamp the report with the revision it actually graded. A report that does not name its commit
  # is the same defect as a measurement that does not name its pin.
  if [[ -f "$OUT/gate_report.json" ]] && command -v jq >/dev/null 2>&1; then
    jq --arg rev "$SHA" --arg short "$SHORT" '. + {graded_revision: $rev, graded_revision_short: $short}' \
      "$OUT/gate_report.json" > "$OUT/gate_report.json.tmp" && mv "$OUT/gate_report.json.tmp" "$OUT/gate_report.json"
  fi
fi

# Refuse to say "graded" when nothing was graded.
#
# The first run of this script printed `graded b731a49 -> ... (rc=1)` having produced NO report at
# all — the suite died on argument parsing before its first line. A wrapper that reports a grade it
# did not obtain is the exact defect class this repo has spent the day closing, and it appeared in
# the tool written to grade the graders. A missing report is now a hard error with its own code.
if [[ ! -f "$OUT/gate_report.json" ]]; then
  echo "ANUBIS_AUDIT_HEAD_NO_REPORT: the suite produced no gate_report.json for $SHORT." >&2
  echo "  Nothing was graded. Do not read the exit code as a verdict." >&2
  exit 3
fi

echo "[audit-head] graded $SHORT -> $OUT (rc=$RC)"
exit $RC
