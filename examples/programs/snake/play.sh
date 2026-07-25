#!/usr/bin/env bash
# Instant-feel launcher for pure Anubis Snake.
# `anubis run` recompiles every time and prints NOTHING until rustc finishes
# (~1–3s) — that looks like a hang. This script prints immediately and uses a
# cached native binary so the board appears right away.
set -euo pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
SRC="$DIR/snake.anb"
OUT_DIR="$DIR/bin"
BIN="$OUT_DIR/snake"
ANUBIS="$ROOT/target/release/anubis"

echo "Anubis Snake"
echo "------------"

if [[ ! -f "$SRC" ]]; then
  echo "error: missing $SRC" >&2
  exit 1
fi

need_build=0
if [[ ! -x "$BIN" ]]; then
  need_build=1
elif [[ "$SRC" -nt "$BIN" ]]; then
  need_build=1
fi

if [[ "$need_build" -eq 1 ]]; then
  if [[ ! -x "$ANUBIS" ]]; then
    echo "error: Anubis binary not found at:" >&2
    echo "  $ANUBIS" >&2
    echo "Build it first, then re-run this script." >&2
    exit 1
  fi
  echo "Building native binary (one-time, ~2s)..."
  mkdir -p "$OUT_DIR"
  "$ANUBIS" build "$SRC" --out "$OUT_DIR"
  # build emits anubis_out — pin a stable name
  if [[ -x "$OUT_DIR/anubis_out" ]]; then
    install -m 0755 "$OUT_DIR/anubis_out" "$BIN"
  else
    echo "error: build did not produce $OUT_DIR/anubis_out" >&2
    exit 1
  fi
  echo "Build done."
else
  echo "Using cached binary: $BIN"
fi

echo ""
echo "Controls: w/a/s/d + Enter | empty Enter = hold | batch ok (ddd) | q = quit"
echo "Starting..."
echo ""

exec "$BIN" "$@"
