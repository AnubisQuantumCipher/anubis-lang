#!/usr/bin/env bash
set -euo pipefail

# Publish an IMMUTABLE, content-addressed snapshot of the release binary for agents to measure.
#
# THE PROBLEM THIS SOLVES
#
# `cargo build` rewrites `target/release/anubis` IN PLACE. When the lead rebuilds while an agent is
# mid-round, the agent's measurements straddle two different compilers. Recording a sha256 at the
# start of a round does not help: the path it names has already changed underneath by the time the
# round ends. In one session an adversary agent legitimately recorded THREE different pins across two
# rounds and had to re-run everything twice, and the lead misread its own fix as non-working.
#
# A content-addressed copy cannot be mutated by a rebuild — a new build produces a NEW filename, so
# any agent holding the old path keeps measuring exactly what it started with. That is the whole
# idea; everything below is bookkeeping.
#
# USAGE
#
#   scripts/publish_pin.sh                 # snapshot the current release binary, print its path
#   scripts/publish_pin.sh --current       # print the path of the current pin (no build, no copy)
#
# AGENTS: at the START of a round, resolve the pin ONCE and use it for the whole round:
#
#   ANUBIS_BIN="$(scripts/publish_pin.sh --current)"
#   "$ANUBIS_BIN" check foo.anb
#
# Then report that exact path plus its sha256. If the lead publishes a new pin mid-round, your old
# pin still exists and still works — finish the round on it, then re-measure deliberately rather than
# discovering the change in your results.
#
# The lead publishes a new pin after every build that agents should see, and SAYS SO. Silent
# republication is the failure mode this replaces.

PIN_DIR="vm/pins"
CURRENT="$PIN_DIR/CURRENT"
SRC="target/release/anubis"

if [[ "${1:-}" == "--current" ]]; then
  if [[ ! -f "$CURRENT" ]]; then
    echo "no pin published yet — run scripts/publish_pin.sh" >&2
    exit 1
  fi
  pin="$(cat "$CURRENT")"
  if [[ ! -x "$pin" ]]; then
    echo "CURRENT names a missing pin: $pin" >&2
    exit 1
  fi
  echo "$pin"
  exit 0
fi

# Record the SOURCE-TREE hash alongside the binary hash.
#
# The staleness guard above catches a binary OLDER than its sources. It cannot catch the inverse
# failure, which bit this session three times: `cargo build` reporting "Finished in 0.15s" — claiming
# up to date — when the last real compile predates an edit. The binary is then NEWER than the
# sources by mtime and still does not contain them, so every mtime-based check passes and an agent
# scores a fix that is not in the binary. It was caught only because a "rebuilt" binary had a
# content-hash IDENTICAL to the previous pin, which cannot happen after a real source change.
#
# With the source hash in the .meta, anyone can ask the question that actually matters:
#   does this pin correspond to the tree I am looking at?
# `--verify` answers it in one command.
src_tree_hash() {
  find compiler/src tools/anubis/src solver/src compiler/stdlib \
    -type f \( -name '*.rs' -o -name '*.anb' \) 2>/dev/null \
    | LC_ALL=C sort \
    | xargs shasum -a 256 2>/dev/null \
    | shasum -a 256 | cut -d' ' -f1
}

if [[ "${1:-}" == "--verify" ]]; then
  if [[ ! -f "$CURRENT" ]]; then echo "no pin published yet" >&2; exit 1; fi
  pin="$(cat "$CURRENT")"
  # `|| true` is load-bearing: grep exits 1 on no-match, and under `set -euo pipefail` that kills
  # the assignment before the diagnostic below can print — the guard would fail SILENTLY, which is
  # the exact defect it exists to catch. This is the third time in one session that this shape was
  # written by the person auditing it.
  recorded="$(grep -E '^src_tree:' "$pin.meta" 2>/dev/null | awk '{print $2}' || true)"
  actual="$(src_tree_hash || true)"
  if [[ -z "$recorded" ]]; then
    echo "STALE-UNKNOWN: $pin has no src_tree hash (published before this guard existed)" >&2
    exit 2
  fi
  if [[ "$recorded" != "$actual" ]]; then
    echo "PIN DOES NOT MATCH THE TREE" >&2
    echo "  pin:        $pin" >&2
    echo "  pin src:    $recorded" >&2
    echo "  actual src: $actual" >&2
    echo "The binary was NOT built from the current sources. Rebuild before measuring." >&2
    exit 1
  fi
  echo "pin matches tree: $pin"
  exit 0
fi

if [[ ! -x "$SRC" ]]; then
  echo "no release binary at $SRC — build first (lead only; agents must not run cargo build)" >&2
  exit 1
fi

# Warn when the CURRENT pin is older than the binary that would replace it. Publishing is manual by
# design (agents must not see the instrument change mid-round), but FORGETTING to publish after a
# build silently measures stale code — a gate then grades a compiler that predates the fix under
# test and reports failures that are already fixed. That happened once; this is the tell.
if [[ -f "$CURRENT" ]]; then
  prev="$(cat "$CURRENT")"
  if [[ -f "$prev" && "$SRC" -nt "$prev" ]]; then
    echo "note: $SRC is newer than the current pin ($prev) — publishing a new one" >&2
  fi
fi

# FAIL CLOSED: a binary older than its own sources cannot contain them.
#
# This is not hypothetical. On 2026-07-28 a patch failed to compile, cargo left the PREVIOUS binary
# in place, and the mtime check above happily called it "newer than the current pin" and published
# it. The pin was then named as the post-fix instrument for a fix it did not contain. An agent
# scoring on it would have measured zero flips, been correct about the measurement, and wrong about
# the world — and the conclusion would have been "the fix is dead code" rather than "the build
# failed". A stale instrument that LOOKS fresh is worse than an obviously missing one.
#
# Escape hatch is explicit and loud, never silent: ANUBIS_PIN_ALLOW_STALE=1.
if [[ "${ANUBIS_PIN_ALLOW_STALE:-0}" != "1" ]]; then
  stale_src=""
  for d in compiler/src tools/anubis/src solver/src compiler/stdlib; do
    [[ -d "$d" ]] || continue
    found="$(find "$d" -type f \( -name '*.rs' -o -name '*.anb' \) -newer "$SRC" -print 2>/dev/null | head -1)"
    if [[ -n "$found" ]]; then stale_src="$found"; break; fi
  done
  if [[ -n "$stale_src" ]]; then
    echo "REFUSING to publish a stale pin." >&2
    echo "  $stale_src" >&2
    echo "  is NEWER than $SRC" >&2
    echo "The binary predates its own source: the last build failed, or was never run." >&2
    echo "Build successfully first. Override only if you know why: ANUBIS_PIN_ALLOW_STALE=1" >&2
    exit 1
  fi
fi

mkdir -p "$PIN_DIR"
sha="$(shasum -a 256 "$SRC" | cut -d' ' -f1)"
short="${sha:0:12}"
pin="$PIN_DIR/anubis-$short"

# Content-addressed, so an identical rebuild is a no-op rather than a churn event.
if [[ ! -f "$pin" ]]; then
  # Copy to a temp name in the same directory and rename, so a reader can never observe a
  # half-written pin. Renaming within one filesystem is atomic.
  tmp="$pin.tmp.$$"
  cp "$SRC" "$tmp"
  chmod +x "$tmp"
  mv "$tmp" "$pin"
fi
chmod a-w "$pin" 2>/dev/null || true

echo "$pin" > "$CURRENT.tmp.$$" && mv "$CURRENT.tmp.$$" "$CURRENT"

{
  echo "pin:    $pin"
  echo "sha256: $sha"
  echo "source: $SRC"
  echo "head:   $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "utc:    $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  echo "src_tree: $(src_tree_hash)"
} | tee "$pin.meta"

echo
echo "agents: ANUBIS_BIN=\"\$(scripts/publish_pin.sh --current)\""
