#!/usr/bin/env bash
# Shadow-mode corpus diff — the safety net for the type-system phase.
#
# A new static check (bidirectional inference, captured generics, trait coherence, typed `?`) is
# added in SHADOW mode first: it emits through SemanticContext::emit(.., shadow_gated=true), so with
# ANUBIS_SHADOW_TYPES=1 its would-be rejections are logged to stderr as `ANUBIS_SHADOW: <code> <msg>`
# WITHOUT entering the enforcing `diagnostics` Err-gate — no program is rejected.
#
# This driver runs `anubis check` with shadow on over the WHOLE corpus and classifies every
# would-be rejection against the fixture's declared intent:
#   EXPECTED   — the file carries `// EXPECT: FAIL` and the shadow diagnostic matches its
#                `// ERROR_CONTAINS:` needle (the check would reject a program that SHOULD fail).
#   UNEXPECTED — anything else: a currently-accepted program the new check would newly reject.
#
# Promotion criterion: a check is flipped from shadow to enforcing (`shadow_gated=false`) ONLY when
# this driver reports UNEXPECTED = 0. That is how additive rejection power lands without breaking a
# single working program. Fail-closed: exits non-zero if UNEXPECTED > 0.
#
# With no check yet routed through emit, the corpus yields ZERO shadow diagnostics ⇒ UNEXPECTED = 0
# ⇒ PASS. That is the correct inert baseline proving the harness is wired and silent.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
BIN="${ANUBIS_BIN:-./target/release/anubis}"
OUT="${1:-out/shadow_diff}"
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi
rm -rf "$OUT"; mkdir -p "$OUT"

if [[ ! -x "$BIN" ]]; then
  echo "SHADOW_DIFF: FAIL (no release binary at $BIN — run: cargo build --release -p anubis)"; exit 1
fi

# Corpus: every .anb the checker can see, excluding worktree copies and build scratch.
# (while-read, not mapfile — macOS ships bash 3.2 which lacks mapfile.)
FILES=()
while IFS= read -r _f; do FILES+=("$_f"); done < <(find examples tests/fixtures selfhost/corpus selfhost/src compiler/stdlib \
  -name '*.anb' -not -path '*/.claude/*' -not -path '*/out/*' 2>/dev/null | sort)

total_shadow=0
expected=0
unexpected=0
: >"$OUT/unexpected.txt"
: >"$OUT/expected.txt"
: >"$OUT/all_shadow.txt"

for f in "${FILES[@]}"; do
  # Work-class timeout invariant (3600s). Shadow mode never rejects, so exit code is ignored here;
  # we only harvest the ANUBIS_SHADOW: lines from stderr.
  err="$OUT/$(echo "$f" | tr '/' '_').err"
  ANUBIS_SHADOW_TYPES=1 timeout 3600 "$BIN" check "$f" >/dev/null 2>"$err" || true
  # Harvest shadow lines (may be none).
  lines=()
  while IFS= read -r _l; do [[ -n "$_l" ]] && lines+=("$_l"); done < <(grep -E '^ANUBIS_SHADOW: ' "$err" 2>/dev/null || true)
  [[ ${#lines[@]} -eq 0 ]] && continue

  expect=$(grep -oE 'EXPECT: [A-Z]+' "$f" 2>/dev/null | head -1 | awk '{print $2}' || echo "PASS")
  needle=$(grep -oE 'ERROR_CONTAINS: .*' "$f" 2>/dev/null | sed 's/ERROR_CONTAINS: //' | head -1 || echo "")

  for ln in "${lines[@]}"; do
    total_shadow=$((total_shadow+1))
    echo "$f :: $ln" >>"$OUT/all_shadow.txt"
    # A shadow diagnostic is EXPECTED iff the fixture is declared FAIL and (no needle, or the
    # diagnostic matches the needle). Everything else is a regression the promotion must not ship.
    if [[ "$expect" == "FAIL" ]] && { [[ -z "$needle" ]] || echo "$ln" | grep -qE "$needle"; }; then
      expected=$((expected+1)); echo "$f :: $ln" >>"$OUT/expected.txt"
    else
      unexpected=$((unexpected+1)); echo "$f :: $ln" >>"$OUT/unexpected.txt"
    fi
  done
done

echo "shadow corpus: ${#FILES[@]} programs scanned"
echo "shadow diagnostics: total=$total_shadow expected=$expected unexpected=$unexpected"
if [[ $unexpected -gt 0 ]]; then
  echo "SHADOW_DIFF: FAIL ($unexpected unexpected would-be rejections — a currently-accepted program"
  echo "would be newly rejected; do NOT promote the check to enforcing). See:"
  echo "  $OUT/unexpected.txt"
  sed 's/^/    /' "$OUT/unexpected.txt" | head -20
  exit 1
fi
echo "SHADOW_DIFF: PASS (unexpected=0 — safe to promote any check that fired only on EXPECT:FAIL fixtures)"
exit 0
