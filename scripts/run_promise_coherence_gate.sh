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
# So every restatement of the PRODUCT promise must carry, near it, BOTH:
#
#   1. a SCOPE QUALIFIER — an explicit statement that green is not totality; and
#   2. a POINTER to docs/CLAIMS.md, where the open-issues list actually lives.
#
# The stronger universal research formulation is not a scoped product promise at all. An asserted
# ``anubis check PASS => the program cannot violate ...`` restatement is therefore rejected rather
# than accepted when surrounded by qualifiers. Research correspondence can be described without
# publishing that universal sentence as a current guarantee.
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

def normalize_markdown_emphasis(text):
    """Remove policy-neutral emphasis without changing line boundaries."""
    return re.sub(r"[*_]", "", text)

# The stronger formulation is deliberately matched as a DIRECT assertion. This does not match a
# neighbouring negation such as "it does not yet mean the program cannot violate" because the
# assertion operator must connect check PASS/passing directly to "the program cannot violate".
UNIVERSAL = re.compile(
    r"`?(?:anubis\s+)?check`?\s+(?:PASS|passing)\s*"
    r"(?:⇒|=>|means(?:\s+that)?)\s+(?:the\s+)?program\s+cannot\s+violate",
    re.I,
)

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
    "does not yet mean",
]
CLAIMS_PTR = re.compile(r"CLAIMS\.md")

WINDOW_BEFORE, WINDOW_AFTER = 4, 20

BASE_SKIP_REASONS = {
    ".git": "version-control internals, not repository documentation",
    ".claude": "local agent control material, not published product documentation",
    "target": "generated build output",
    "node_modules": "installed third-party dependencies",
    "out": "generated gate output and planted negative-control fixtures",
    "vm": "generated VM evidence and immutable binary metadata",
    "adversary": "adversarial work products that preserve falsified quotations",
}
POLICY_SKIP_REASONS = {
    ".hermes": "local agent attachments/session material, not a shipped claim surface",
    "scratchpad": "disposable experiments and audit records that preserve falsified quotations",
    "implementer": "internal execution receipts/work products, not product documentation",
    "vendor": "third-party vendored documentation outside Anubis claim ownership",
}
SKIP_REASONS = dict(BASE_SKIP_REASONS)
SKIP_REASONS.update(POLICY_SKIP_REASONS)
skip_dirs = set(SKIP_REASONS)

findings, scan_errors, product_checked, universal_checked = [], [], 0, 0
excluded_roots = []
if not os.path.isdir(root) or os.path.islink(root):
    scan_errors.append({"file": root, "error": "scan root is not a real directory"})

def walk_error(exc):
    scan_errors.append({"file": getattr(exc, "filename", root) or root, "error": str(exc)})

for dirpath, dirnames, filenames in os.walk(root, onerror=walk_error):
    kept = []
    for dirname in dirnames:
        if dirname in skip_dirs:
            excluded_roots.append((os.path.join(dirpath, dirname), dirname))
        else:
            kept.append(dirname)
    dirnames[:] = kept

    rel_dir = os.path.relpath(dirpath, root)
    if rel_dir == os.path.join("docs", "history"):
        excluded_roots.append((dirpath, "docs/history"))
        dirnames[:] = []
        continue
    for fn in filenames:
        if not fn.endswith(".md"):
            continue
        path = os.path.join(dirpath, fn)
        rel = os.path.relpath(path, root)
        try:
            lines = open(path, encoding="utf-8").read().splitlines()
        except (OSError, UnicodeDecodeError) as exc:
            scan_errors.append({"file": rel, "error": str(exc)})
            continue

        # Reject the universal sentence as an asserted guarantee. Scan the full file so a wrapped
        # assertion is found exactly once; overlapping two-line windows double-count whenever the
        # assertion begins on the second line of the first window.
        joined = "\n".join(lines)
        normalized_joined = normalize_markdown_emphasis(joined)
        for match in UNIVERSAL.finditer(normalized_joined):
            universal_checked += 1
            line_no = normalized_joined.count("\n", 0, match.start()) + 1
            findings.append({
                "file": rel,
                "line": line_no,
                "missing": ["banned-universal-restatement"],
            })

        for i, line in enumerate(lines):
            if not PROMISE.search(line):
                continue
            product_checked += 1
            lo = max(0, i - WINDOW_BEFORE)
            hi = min(len(lines), i + WINDOW_AFTER + 1)
            window = "\n".join(lines[lo:hi])
            # Markdown emphasis must not change policy semantics: ``does **not** yet mean`` is the
            # same qualifier as plain text. Strip only formatting markers, not words/punctuation.
            wl = re.sub(r"[`*_]", "", window.lower())
            has_q = any(q.lower() in wl for q in QUALIFIERS)
            has_p = bool(CLAIMS_PTR.search(window))
            missing = []
            if not has_q:
                missing.append("scope-qualifier")
            if not has_p:
                missing.append("CLAIMS.md-pointer")
            if missing:
                findings.append({"file": rel, "line": i + 1, "missing": missing})

# Exclusions are policy, not invisibility. Inventory every excluded Markdown tree with BOTH live
# matchers and report both narrowed columns on every run. These observations do not become live
# findings: historical/audit quotations and third-party docs are not product claims. But widening
# this list can no longer turn on-disk product or universal forms into a silent headline zero.
excluded_universal_by_reason = {reason: 0 for reason in SKIP_REASONS}
excluded_universal_by_reason["docs/history"] = 0
excluded_product_by_reason = {reason: 0 for reason in SKIP_REASONS}
excluded_product_by_reason["docs/history"] = 0
for excluded_root, reason in excluded_roots:
    for dirpath, _, filenames in os.walk(excluded_root, onerror=walk_error):
        for fn in filenames:
            if not fn.endswith(".md"):
                continue
            path = os.path.join(dirpath, fn)
            try:
                text = open(path, encoding="utf-8").read()
            except (OSError, UnicodeDecodeError) as exc:
                scan_errors.append({"file": os.path.relpath(path, root), "error": str(exc)})
                continue
            normalized_text = normalize_markdown_emphasis(text)
            excluded_universal_by_reason[reason] += len(
                list(UNIVERSAL.finditer(normalized_text))
            )
            excluded_product_by_reason[reason] += sum(
                1 for line in text.splitlines() if PROMISE.search(line)
            )

excluded_universal = sum(excluded_universal_by_reason.values())
policy_excluded_universal = sum(
    excluded_universal_by_reason[name] for name in POLICY_SKIP_REASONS
)
excluded_product = sum(excluded_product_by_reason.values())
policy_excluded_product = sum(
    excluded_product_by_reason[name] for name in POLICY_SKIP_REASONS
)

result = {
    "checked": product_checked + universal_checked,
    "product_restatements": product_checked,
    "repo_product_restatements": product_checked + excluded_product,
    "excluded_product_restatements": excluded_product,
    "policy_excluded_product_restatements": policy_excluded_product,
    "excluded_product_by_reason": excluded_product_by_reason,
    "universal_assertions": universal_checked,
    "repo_universal_assertions": universal_checked + excluded_universal,
    "excluded_universal_assertions": excluded_universal,
    "policy_excluded_universal_assertions": policy_excluded_universal,
    "excluded_universal_by_reason": excluded_universal_by_reason,
    "policy_skip_reasons": POLICY_SKIP_REASONS,
    "findings": findings,
    "scan_errors": scan_errors,
    "claims_md": {},
}

if require_claims_md:
    cm = os.path.join(root, "docs", "CLAIMS.md")
    ok_framing, open_rows = False, 0
    if os.path.exists(cm):
        try:
            text = open(cm, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError) as exc:
            scan_errors.append({"file": os.path.relpath(cm, root), "error": str(exc)})
            text = ""
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
  printf 'The promise: check passing means Anubis found no way for the program to violate\nits contracts.\n\nIt does **not** yet mean the program cannot violate that policy. See docs/CLAIMS.md for the open issues.\n' \
    > "$TMP/docs/ok.md"
  rm -f "$TMP/docs/bare.md"
  good="$(scan "$TMP" 0)"
  n_good="$(python3 -c 'import json,sys;d=json.loads(sys.argv[1]);print(len(d["findings"]))' "$good")"
  c_good="$(python3 -c 'import json,sys;d=json.loads(sys.argv[1]);print(d["checked"])' "$good")"

  # (c) the universal research promise stated as a current guarantee -> must be reported even
  # though it does not contain the product promise's distinctive "found no way" clause.
  mkdir -p "$TMP/universal/docs"
  printf '## Claimed guarantee\n`anubis check` PASS means the program cannot violate its stated contracts, effects, capabilities, or\ninformation-flow policy at runtime.\n' \
    > "$TMP/universal/docs/universal.md"
  universal="$(scan "$TMP/universal" 0)"
  n_universal="$(python3 -c 'import json,sys;d=json.loads(sys.argv[1]);print(len(d["findings"]))' "$universal")"

  # (c2) Markdown emphasis is presentation, not policy. Adding exactly two `*` characters around
  # PASS must not turn the same asserted universal sentence from a finding into a clean document.
  printf '## Claimed guarantee\n`anubis check` *PASS* means the program cannot violate its contracts.\n' \
    > "$TMP/universal/docs/universal.md"
  emphasized_universal="$(scan "$TMP/universal" 0)"
  n_emphasized_universal="$(python3 -c 'import json,sys;d=json.loads(sys.argv[1]);print(len(d["findings"]))' "$emphasized_universal")"

  # (d) every policy exclusion must remain visible in the returned inventory. The quotations are
  # not live findings, but a new skip can no longer turn them into an unexplained zero.
  mkdir -p "$TMP/skips/.hermes" "$TMP/skips/scratchpad" "$TMP/skips/implementer" "$TMP/skips/vendor"
  for d in .hermes scratchpad implementer vendor; do
    printf '`anubis check` *PASS* means the program cannot violate its contracts.\n' \
      > "$TMP/skips/$d/quoted.md"
    printf 'check passing means Anubis found no way for the program to violate its contracts.\n' \
      > "$TMP/skips/$d/product.md"
  done
  skipped="$(scan "$TMP/skips" 0)"
  skip_ok="$(python3 -c '
import json,sys
d=json.loads(sys.argv[1])
universal_counts=d.get("excluded_universal_by_reason") or {}
product_counts=d.get("excluded_product_by_reason") or {}
reasons=d.get("policy_skip_reasons") or {}
names=(".hermes","scratchpad","implementer","vendor")
ok=(d.get("universal_assertions")==0 and
    d.get("repo_universal_assertions")==4 and
    d.get("excluded_universal_assertions")==4 and
    d.get("policy_excluded_universal_assertions")==4 and
    d.get("product_restatements")==0 and
    d.get("repo_product_restatements")==4 and
    d.get("excluded_product_restatements")==4 and
    d.get("policy_excluded_product_restatements")==4 and
    all(universal_counts.get(name)==1 and product_counts.get(name)==1 and
        bool(reasons.get(name)) for name in names))
print(1 if ok else 0)
' "$skipped")"

  rm -rf "$TMP"
  if [[ "$n_bare" -lt 1 ]]; then
    echo "PROMISE_COHERENCE_GATE: FAIL (self-test: an unqualified promise was NOT reported)"
    exit 1
  fi
  if [[ "$n_good" -ne 0 || "$c_good" -ne 1 ]]; then
    echo "PROMISE_COHERENCE_GATE: FAIL (self-test: a properly qualified promise was reported, or not seen)"
    exit 1
  fi
  if [[ "$n_universal" -ne 1 ]]; then
    echo "PROMISE_COHERENCE_GATE: FAIL (self-test: an asserted universal promise was not reported exactly once; got $n_universal)"
    exit 1
  fi
  if [[ "$n_emphasized_universal" -ne 1 ]]; then
    echo "PROMISE_COHERENCE_GATE: FAIL (self-test: Markdown emphasis bypassed the universal matcher; got $n_emphasized_universal)"
    exit 1
  fi
  if [[ "$skip_ok" -ne 1 ]]; then
    echo "PROMISE_COHERENCE_GATE: FAIL (self-test: skip policy did not disclose both excluded promise columns and justifications)"
    exit 1
  fi
  echo "PROMISE_COHERENCE_GATE: PASS (self-test: unqualified and emphasized universal reported, qualified product promise clean)"
  echo "PROMISE_COHERENCE_SELFTEST: PASS (four policy skip roots disclose product and universal counts with reasons)"
  exit 0
fi

RESULT="$(scan "$SCAN_ROOT" 1)"
scan_rc=$?
if [[ $scan_rc -ne 0 ]]; then
  echo "PROMISE_COHERENCE_GATE: FAIL (scanner exited $scan_rc before producing a verdict)" >&2
  exit 1
fi
CHECKED="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["checked"])' "$RESULT")"
RESTATEMENTS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["product_restatements"])' "$RESULT")"
REPO_PRODUCT_RESTATEMENTS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["repo_product_restatements"])' "$RESULT")"
EXCLUDED_PRODUCT_RESTATEMENTS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["excluded_product_restatements"])' "$RESULT")"
POLICY_EXCLUDED_PRODUCT_RESTATEMENTS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["policy_excluded_product_restatements"])' "$RESULT")"
UNIVERSAL_ASSERTIONS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["universal_assertions"])' "$RESULT")"
REPO_UNIVERSAL_ASSERTIONS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["repo_universal_assertions"])' "$RESULT")"
EXCLUDED_UNIVERSAL_ASSERTIONS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["excluded_universal_assertions"])' "$RESULT")"
POLICY_EXCLUDED_UNIVERSAL_ASSERTIONS="$(python3 -c 'import json,sys;print(json.loads(sys.argv[1])["policy_excluded_universal_assertions"])' "$RESULT")"
NFIND="$(python3 -c 'import json,sys;print(len(json.loads(sys.argv[1])["findings"]))' "$RESULT")"
SCAN_ERRORS="$(python3 -c 'import json,sys;print(len(json.loads(sys.argv[1]).get("scan_errors") or []))' "$RESULT")"

echo "product promise restatements checked: $RESTATEMENTS"
echo "repo-wide product promise restatements observed: $REPO_PRODUCT_RESTATEMENTS"
echo "product promise restatements excluded by policy: $EXCLUDED_PRODUCT_RESTATEMENTS"
echo "policy skip roots excluded product promises: $POLICY_EXCLUDED_PRODUCT_RESTATEMENTS"
echo "repo-wide asserted universal forms observed: $REPO_UNIVERSAL_ASSERTIONS"
echo "asserted universal promise forms checked: $UNIVERSAL_ASSERTIONS"
echo "asserted universal forms checked: $UNIVERSAL_ASSERTIONS"
echo "asserted universal forms excluded by policy: $EXCLUDED_UNIVERSAL_ASSERTIONS"
echo "policy skip roots excluded: $POLICY_EXCLUDED_UNIVERSAL_ASSERTIONS"
echo "scan errors: $SCAN_ERRORS"
python3 -c '
import json,sys
d=json.loads(sys.argv[1])
universal=d["excluded_universal_by_reason"]
product=d["excluded_product_by_reason"]
for name, reason in d["policy_skip_reasons"].items():
    print("  %s: universal=%d product=%d — %s" % (
        name, universal.get(name, 0), product.get(name, 0), reason))
' "$RESULT"
python3 -c '
import json,sys
d=json.loads(sys.argv[1])
for f in d["findings"]:
    print("  DRIFT %s:%d missing %s" % (f["file"], f["line"], ", ".join(f["missing"])))
for e in d.get("scan_errors") or []:
    print("  SCAN ERROR %s: %s" % (e.get("file"), e.get("error")))
c=d.get("claims_md") or {}
if c:
    print("  CLAIMS.md: exists=%s framing=%s open_issues_body_lines=%s" % (
        c.get("exists"), c.get("has_known_defects_framing"), c.get("open_issues_body_lines")))
' "$RESULT"

FAILED=$((NFIND + SCAN_ERRORS))
CHECKED=$((CHECKED + SCAN_ERRORS))
# The floor ratchets RESTATEMENTS READ. Capture it before the anchor check can inflate the counter,
# or a broken anchor would quietly move the coverage number too -- a floor that tracks a different
# quantity than its name claims is the same defect this gate exists to catch, one level up.

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

# Coverage ratchet: the number of PRODUCT restatements this gate actually READ must not silently
# shrink. Banned universal assertions are findings, never floor coverage.
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
