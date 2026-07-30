#!/usr/bin/env bash
# Promise-coherence gate — the headline promise must INHERIT the open-issues framing.
#
# `docs/CLAIMS.md` gets this right internally:
#
#     "Green means no KNOWN defects — not no defects."
#
# The failure this gate exists to catch is that the HEADLINE PROMISE, restated in other docs, does
# not inherit that. A reader who meets the promise in AGENTS.md or HANDOFF.md and never reaches
# CLAIMS.md walks away with a stronger claim than the repo can discharge — and nothing mechanical
# stops the two from drifting apart, because they live in different files edited at different times.
#
# So every restatement of the promise must carry, near it, BOTH:
#
#   1. a SCOPE QUALIFIER — an explicit statement that green is not totality; and
#   2. a POINTER to docs/CLAIMS.md, where the open-issues list actually lives.
#
# And `docs/CLAIMS.md` itself must still carry the framing and a NON-EMPTY open-issues section. An
# empty open-issues list would make every qualifier above vacuous: pointing at nothing reads as
# "nothing is open", which is the strongest possible claim wearing the costume of a hedge.
#
# Declared verdict (seal-scored):
#   PROMISE_COHERENCE_GATE: PASS
#   PROMISE_COHERENCE_GATE: FAIL
#
# Usage:
#   bash scripts/run_promise_coherence_gate.sh
#   bash scripts/run_promise_coherence_gate.sh --scan-root path/to/fixture_docs
#   bash scripts/run_promise_coherence_gate.sh --self-test
#
# Bash 3.2 compatible. Does not need ANUBIS_BIN (docs only).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

SCAN_ROOT="$ROOT"
SELF_TEST=0
FLOOR_FILE="$ROOT/scripts/floors/promise_coherence.count_floor"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scan-root) SCAN_ROOT="$(cd "$2" && pwd)"; shift 2 ;;
    --self-test) SELF_TEST=1; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# ---------------------------------------------------------------------------------------------
# The scan. Kept in python3 because the check is "is a qualifier NEAR this line", which is a window
# over lines rather than a line predicate, and expressing that in grep invites the off-by-one that
# would make the gate pass on a doc it never really read.
# ---------------------------------------------------------------------------------------------
scan() {
  local root="$1" require_claims_md="$2"
  python3 - "$root" "$require_claims_md" <<'PY'
import os, re, sys, json

root = sys.argv[1]
require_claims_md = sys.argv[2] == "1"

# The distinctive clause of the promise. Deliberately matches the SHARED core rather than a full
# canonical sentence: the three current restatements differ in wording ("its stated contracts,
# effects, capabilities, or information-flow policy at runtime" vs "its contracts, effects,
# capabilities or information-flow"), and demanding byte-identity would force a cosmetic rewrite of
# living docs to satisfy a gate. What must not drift is the CLAIM, and every restatement of the
# claim contains this clause.
PROMISE = re.compile(r"found no way for the program to violate", re.I)

# A qualifier must SAY that green is not totality. Each of these is an explicit scope limitation
# already present in this repo's docs -- the set is derived from what is written, not invented.
QUALIFIERS = [
    "not a totality claim",
    "no known defects",
    'not "cannot violate"',
    "not “cannot violate”",       # curly quotes
    "absolute totality is not established",
    "is not established",
    "not no defects",
]
CLAIMS_PTR = re.compile(r"CLAIMS\.md")

WINDOW_BEFORE, WINDOW_AFTER = 4, 20

skip_dirs = {".git", "target", "node_modules", ".claude", "out", "vm", "adversary"}
# History is a record of what was true on its seal date; it is not a live restatement and is
# explicitly framed that way in-tree.
skip_path_parts = {os.path.join("docs", "history")}

findings, checked = [], 0
for dirpath, dirnames, filenames in os.walk(root):
    dirnames[:] = [d for d in dirnames if d not in skip_dirs]
    if any(p in dirpath for p in skip_path_parts):
        continue
    for fn in filenames:
        if not fn.endswith(".md"):
            continue
        path = os.path.join(dirpath, fn)
        rel = os.path.relpath(path, root)
        try:
            lines = open(path, encoding="utf-8").read().splitlines()
        except (OSError, UnicodeDecodeError):
            continue
        for i, line in enumerate(lines):
            if not PROMISE.search(line):
                continue
            checked += 1
            lo = max(0, i - WINDOW_BEFORE)
            hi = min(len(lines), i + WINDOW_AFTER + 1)
            window = "\n".join(lines[lo:hi])
            wl = window.lower()
            has_q = any(q.lower() in wl for q in QUALIFIERS)
            has_p = bool(CLAIMS_PTR.search(window))
            missing = []
            if not has_q:
                missing.append("scope-qualifier")
            if not has_p:
                missing.append("CLAIMS.md-pointer")
            if missing:
                findings.append({"file": rel, "line": i + 1, "missing": missing})

result = {"checked": checked, "findings": findings, "claims_md": {}}

if require_claims_md:
    cm = os.path.join(root, "docs", "CLAIMS.md")
    ok_framing, open_rows = False, 0
    if os.path.exists(cm):
        text = open(cm, encoding="utf-8").read()
        ok_framing = "no KNOWN defects" in text
        # The open-issues section must exist and carry content. Emptiness here would make every
        # qualifier elsewhere point at nothing, which reads as "nothing is open".
        m = re.search(r"^## Known open issues.*?$", text, re.M)
        if m:
            rest = text[m.end():]
            nxt = re.search(r"^## ", rest, re.M)
            body = rest[: nxt.start()] if nxt else rest
            open_rows = len([ln for ln in body.splitlines() if ln.strip()])
    result["claims_md"] = {
        "exists": os.path.exists(cm),
        "has_known_defects_framing": ok_framing,
        "open_issues_body_lines": open_rows,
    }

print(json.dumps(result))
PY
}

# ---------------------------------------------------------------------------------------------
# Self-test: RED before GREEN. A gate nobody has watched fail is a gate taken on faith.
# ---------------------------------------------------------------------------------------------
if [[ "$SELF_TEST" -eq 1 ]]; then
  TMP="$(mktemp -d)"
  mkdir -p "$TMP/docs"

  # (a) promise with NEITHER qualifier nor pointer -> must be reported twice over
  printf 'The promise: check passing means Anubis found no way for the program to violate\nits contracts.\n' \
    > "$TMP/docs/bare.md"
  bare="$(scan "$TMP" 0)"
  n_bare="$(python3 -c 'import json,sys;d=json.loads(sys.argv[1]);print(len(d["findings"]))' "$bare")"

  # (b) promise WITH both -> must be clean
  printf 'The promise: check passing means Anubis found no way for the program to violate\nits contracts.\n\nGreen means no KNOWN defects. See docs/CLAIMS.md for the open issues.\n' \
    > "$TMP/docs/ok.md"
  rm -f "$TMP/docs/bare.md"
  good="$(scan "$TMP" 0)"
  n_good="$(python3 -c 'import json,sys;d=json.loads(sys.argv[1]);print(len(d["findings"]))' "$good")"
  c_good="$(python3 -c 'import json,sys;d=json.loads(sys.argv[1]);print(d["checked"])' "$good")"

  rm -rf "$TMP"
  if [[ "$n_bare" -lt 1 ]]; then
    echo "PROMISE_COHERENCE_GATE: FAIL (self-test: an unqualified promise was NOT reported)"
    exit 1
  fi
  if [[ "$n_good" -ne 0 || "$c_good" -ne 1 ]]; then
    echo "PROMISE_COHERENCE_GATE: FAIL (self-test: a properly qualified promise was reported, or not seen)"
    exit 1
  fi
  echo "PROMISE_COHERENCE_GATE: PASS (self-test: unqualified reported, qualified clean)"
  exit 0
fi

RESULT="$(scan "$SCAN_ROOT" 1)"
CHECKED="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["checked"])' "$RESULT")"
NFIND="$(python3 -c 'import json,sys;print(len(json.loads(sys.argv[1])["findings"]))' "$RESULT")"

echo "promise restatements checked: $CHECKED"
python3 -c '
import json,sys
d=json.loads(sys.argv[1])
for f in d["findings"]:
    print("  DRIFT %s:%d missing %s" % (f["file"], f["line"], ", ".join(f["missing"])))
c=d.get("claims_md") or {}
if c:
    print("  CLAIMS.md: exists=%s framing=%s open_issues_body_lines=%s" % (
        c.get("exists"), c.get("has_known_defects_framing"), c.get("open_issues_body_lines")))
' "$RESULT"

FAILED="$NFIND"
# The floor ratchets RESTATEMENTS READ. Capture it before the anchor check can inflate the counter,
# or a broken anchor would quietly move the coverage number too -- a floor that tracks a different
# quantity than its name claims is the same defect this gate exists to catch, one level up.
RESTATEMENTS="$CHECKED"

# CLAIMS.md must still be able to carry the weight every qualifier delegates to it.
CM_OK="$(python3 -c '
import json,sys
c=json.loads(sys.argv[1]).get("claims_md") or {}
print(1 if (c.get("exists") and c.get("has_known_defects_framing") and (c.get("open_issues_body_lines") or 0) > 0) else 0)
' "$RESULT")"
if [[ "$CM_OK" -ne 1 ]]; then
  echo "  ANCHOR FAIL: docs/CLAIMS.md is missing, has lost the 'no KNOWN defects' framing, or its open-issues section is empty"
  FAILED=$((FAILED + 1))
  CHECKED=$((CHECKED + 1))
fi

PASSED=$((CHECKED - FAILED))
[[ "$PASSED" -lt 0 ]] && PASSED=0

set +e
finalize "$CHECKED" "$PASSED" "$FAILED"
FIN_RC=$?
set -e

# Coverage ratchet: the number of restatements this gate actually READ must not silently shrink.
# Deleting the promise from a doc, or rewording it past the matcher, would otherwise register as
# "nothing to report" -- indistinguishable from compliance.
mkdir -p "$(dirname "$FLOOR_FILE")"
set +e
assert_floor "promise_coherence" "$RESTATEMENTS" "$FLOOR_FILE"
FLOOR_RC=$?
set -e
if [[ "$FLOOR_RC" -ne 0 ]]; then
  echo "  FLOOR: FAIL ($GATE_FLOOR_ERROR)"
fi

if [[ "$FIN_RC" -eq 0 && "$FLOOR_RC" -eq 0 ]]; then
  echo "PROMISE_COHERENCE_GATE: PASS ($CHECKED restatements, each carrying scope + CLAIMS.md pointer)"
  exit 0
fi
echo "PROMISE_COHERENCE_GATE: FAIL (${GATE_FINAL_REASON:-coverage floor})"
exit 1
