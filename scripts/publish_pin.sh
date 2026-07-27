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

if [[ ! -x "$SRC" ]]; then
  echo "no release binary at $SRC — build first (lead only; agents must not run cargo build)" >&2
  exit 1
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
} | tee "$pin.meta"

echo
echo "agents: ANUBIS_BIN=\"\$(scripts/publish_pin.sh --current)\""
