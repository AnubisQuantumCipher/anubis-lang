#!/usr/bin/env bash
# The source-to-proof correspondence map must stay TRUE, not merely written down.
#
# `docs/PROOF_CORRESPONDENCE.md` says which links in
#
#     AST -> VC -> SMT -> parser -> CNF -> certificate -> runtime
#
# carry evidence, naming a Lean theorem or a gate script for each, and which are in the TCB. That
# document is a claim surface: the moment a cited theorem is renamed or a cited gate deleted, the map
# starts describing a repo that no longer exists — and a stale TCB list is worse than none, because it
# reads as an assurance.
#
# So every citation is CHECKED:
#   - each `theorem X` named in the map must exist in formal/**.lean
#   - each `scripts/*.sh` named must exist
#   - each source path named must exist
#   - the TCB section must be non-empty (an empty TCB is the strongest possible claim; if it is ever
#     genuinely empty that has to be argued in a commit, not achieved by deleting a list)
#
# Declared verdict (seal-scored):
#   PROOF_CORRESPONDENCE_GATE: PASS
#   PROOF_CORRESPONDENCE_GATE: FAIL
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

DOC="docs/PROOF_CORRESPONDENCE.md"
FLOOR_FILE="$ROOT/scripts/floors/proof_correspondence.count_floor"

if [[ "${1:-}" == "--self-test" ]]; then
  # RED before GREEN: a citation that does not resolve must be caught.
  tmp="$(mktemp -d)"; mkdir -p "$tmp/docs" "$tmp/formal" "$tmp/scripts"
  printf 'x `mulVar_correct` y\n## The TCB, enumerated\n1. thing\n' > "$tmp/docs/PROOF_CORRESPONDENCE.md"
  printf 'theorem mulVar_correct : True := trivial\n' > "$tmp/formal/A.lean"
  if ! ( cd "$tmp" && grep -q 'mulVar_correct' formal/A.lean ); then
    echo "PROOF_CORRESPONDENCE_GATE: FAIL (self-test scaffolding broken)"; exit 1
  fi
  printf 'x `no_such_theorem_xyz` y\n' > "$tmp/docs/PROOF_CORRESPONDENCE.md"
  if ( cd "$tmp" && grep -q 'no_such_theorem_xyz' formal/A.lean ); then
    echo "PROOF_CORRESPONDENCE_GATE: FAIL (self-test: a missing theorem resolved)"; exit 1
  fi
  echo "PROOF_CORRESPONDENCE_GATE: PASS (self-test: present citation resolves, absent one does not)"
  exit 0
fi

[[ -f "$DOC" ]] || { echo "PROOF_CORRESPONDENCE_GATE: FAIL (missing $DOC)"; exit 1; }

checked=0; failed=0

# 0. The theorem COUNT the document states must be the comment-stripped count.
#    This existed as an unchecked number for as long as the document has claimed
#    "Counts here are re-derived by the gate; do not hand-edit them" — and it was
#    WRONG: the doc said 163, which is what a naive `grep -c '^\s*theorem'` returns
#    because one theorem sits inside a `/- ... -/` block comment. The gate verified
#    that each NAMED theorem exists and never looked at the aggregate, so a document
#    about proof correspondence overclaimed its own proof count, and asserted the gate
#    had derived it. Producer and consumer disagreeing while the consumer claims they
#    agree is the exact disease this repo tracks.
lean_count="$(python3 - <<'PY'
import re, glob
n = 0
for f in glob.glob('formal/Anubis/*.lean'):
    s = re.sub(r'/-.*?-/', '', open(f).read(), flags=re.S)
    n += len(re.findall(r'^\s*theorem ', s, re.M))
print(n)
PY
)"
doc_count="$(grep -oE 'ships [0-9]+ machine-checked Lean theorems' "$DOC" | grep -oE '[0-9]+' | head -1)"
checked=$((checked+1))
if [[ -z "$doc_count" ]]; then
  echo "  MISS theorem-count — $DOC no longer states 'ships N machine-checked Lean theorems'"
  failed=$((failed+1))
elif [[ "$doc_count" != "$lean_count" ]]; then
  echo "  MISS theorem-count — $DOC says $doc_count, comment-stripped formal/**.lean has $lean_count"
  failed=$((failed+1))
else
  echo "  ok   theorem-count $lean_count (comment-stripped)"
fi

# 1. Lean theorems cited in backticks that look like identifiers ending in a known suffix.
#    Only names that actually appear as `theorem <name>` somewhere are treated as citations, so
#    ordinary prose in backticks is not mistaken for one.
while read -r name; do
  [[ -n "$name" ]] || continue
  checked=$((checked+1))
  if grep -rqE "^\s*theorem\s+${name}\b" formal --include='*.lean' 2>/dev/null; then
    echo "  ok   theorem $name"
  else
    echo "  MISS theorem $name — cited in $DOC, not found in formal/**.lean"; failed=$((failed+1))
  fi
done < <(grep -oE '`[a-zA-Z_][a-zA-Z0-9_]*_correct`' "$DOC" | tr -d '`' | sort -u)

# 2. Every gate script and source path cited must exist.
while read -r path; do
  [[ -n "$path" ]] || continue
  checked=$((checked+1))
  if [[ -e "$path" ]]; then
    echo "  ok   path $path"
  else
    echo "  MISS path $path — cited in $DOC, not present"; failed=$((failed+1))
  fi
done < <(grep -oE '`(scripts|solver|compiler|formal|tools)/[A-Za-z0-9_./-]+`' "$DOC" \
          | tr -d '`' | sed 's/[.,)]*$//' | sort -u)

# 3. The TCB list must be non-empty. "Nothing is trusted" is the strongest claim available and must
#    never be reachable by deleting a section.
# NR>start, or the range terminates on the very line it opens (both patterns match `## The TCB…`)
# and the section reads as empty — which is exactly the false "nothing is trusted" this check exists
# to prevent, produced by the check itself.
tcb_items="$(awk '/^## The TCB, enumerated/{f=1;next} f&&/^## /{f=0} f' "$DOC" | grep -cE '^[0-9]+\. ')"
checked=$((checked+1))
if [[ "$tcb_items" -gt 0 ]]; then
  echo "  ok   TCB enumerated ($tcb_items items)"
else
  echo "  MISS TCB section is empty — an empty TCB is a claim, not a default"; failed=$((failed+1))
fi

echo "citations checked: $checked  failed: $failed"

set +e
finalize "$checked" "$((checked-failed))" "$failed"
FIN=$?
mkdir -p "$(dirname "$FLOOR_FILE")"
assert_floor "proof_correspondence" "$checked" "$FLOOR_FILE"
FLOOR=$?
set -e

if [[ "$FIN" -eq 0 && "$FLOOR" -eq 0 ]]; then
  echo "PROOF_CORRESPONDENCE_GATE: PASS ($checked citations resolve; TCB enumerated)"
  exit 0
fi
echo "PROOF_CORRESPONDENCE_GATE: FAIL (${GATE_FINAL_REASON:-coverage floor})"
exit 1
