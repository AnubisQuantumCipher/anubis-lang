#!/usr/bin/env bash
set -uo pipefail
# guard_reachability.sh — PASS wrong-reason discriminator for must-stay-ACCEPT pure guards.
#
#   Usage: guard_reachability.sh NAME PURE.anb POISON.anb
#          guard_reachability.sh --self-test
#   Env:   ANUBIS_BIN
#
#   exit 0  REACHES   pure ACCEPT + poison REJECT — analysis reaches the shape; pure PASS is real
#   exit 2  BLIND     pure ACCEPT + poison ACCEPT — analysis never charged; PASS protects nothing
#   exit 3  MALFORMED pure REJECT or files missing — not a pure guard, or instrument broken
#
# Dual of fixture_preflight.sh: preflight asks "is ACCEPT a finding?"; this asks
# "is PASS a real all-clear?" A pure twin that stays ACCEPT when poisoned with a
# known-forbidden effect is a false all-clear (wrong-reason PASS).
#
# Deliberately no `set -e`: poison REJECT is DATA (rc!=0), not script failure.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

resolve_bin() {
  if [[ -n "${ANUBIS_BIN:-}" ]]; then echo "$ANUBIS_BIN"; return 0; fi
  local pin; pin="$(bash scripts/publish_pin.sh --current 2>/dev/null)"
  if [[ -n "$pin" && -x "$pin" ]]; then echo "$pin"; return 0; fi
  echo "./target/release/anubis"
}

verdict_of() {
  local bin="$1" file="$2" rc=0
  "$bin" check "$file" >/dev/null 2>&1 || rc=$?
  echo "$rc"
}

reach() {
  local bin="$1" name="$2" pure="$3" poison="$4"
  for f in "$pure" "$poison"; do
    if [[ ! -f "$f" ]]; then echo "MALFORMED missing_file:$f"; return 3; fi
  done
  local prc xrc
  prc="$(verdict_of "$bin" "$pure")"
  xrc="$(verdict_of "$bin" "$poison")"
  if [[ "$prc" -ne 0 ]]; then
    echo "MALFORMED pure_REJECT(rc=$prc) — not a must-stay-ACCEPT guard"
    return 3
  fi
  if [[ "$xrc" -eq 0 ]]; then
    echo "BLIND pure_ACCEPT poison_ACCEPT — analysis does not reach this shape; PASS is wrong-reason"
    return 2
  fi
  echo "REACHES pure_ACCEPT poison_REJECT(rc=$xrc) — analysis reaches; pure PASS is real"
  return 0
}

if [[ "${1:-}" == "--self-test" ]]; then
  BIN="$(resolve_bin)"
  TD="$(mktemp -d)"; trap 'rm -rf "$TD"' EXIT
  fails=0
  ck() {
    if [[ "$2" != "$3" ]]; then echo "  SELFTEST FAIL: $1 expected rc=$2 got rc=$3"; fails=$((fails+1));
    else echo "  ok: $1 (rc=$3)"; fi
  }
  # REACHES: pure clean, poison has write
  cat > "$TD/pure.anb" <<'EOF'
fn pure_fn(p: string, x: string) { }
fn app(f: any) { f("/tmp/pf.txt", "x"); }
fn main() { app(pure_fn); }
EOF
  cat > "$TD/poison.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(f: any) { f("/tmp/pf.txt", "x"); }
fn main() { app(leak); }
EOF
  rc=0; reach "$BIN" t "$TD/pure.anb" "$TD/poison.anb" >/dev/null || rc=$?
  ck "direct apply reaches" 0 "$rc"

  # BLIND: both pure and poison accept because shape is open launder (use elem alias)
  cat > "$TD/blind_pure.anb" <<'EOF'
fn pure_fn(p: string, x: string) { }
fn app(xs: any) { let ys = xs; ys[0]("/tmp/pf.txt", "x"); }
fn main() { app([pure_fn]); }
EOF
  cat > "$TD/blind_poison.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(xs: any) { let ys = xs; ys[0]("/tmp/pf.txt", "x"); }
fn main() { app([leak]); }
EOF
  rc=0; reach "$BIN" t "$TD/blind_pure.anb" "$TD/blind_poison.anb" >/dev/null || rc=$?
  # On current pin elem-alias is YES_OPEN so poison ACCEPTS → BLIND rc=2
  if [[ "$rc" -eq 1 ]]; then
    echo "  SELFTEST FAIL: aborted with undefined rc=1"; fails=$((fails+1))
  else
    echo "  ok: defined verdict on launder shape (rc=$rc, expect 2 if still open)"
  fi

  # MALFORMED pure rejects
  cat > "$TD/notpure.anb" <<'EOF'
fn main() { write_file("/tmp/x","y"); }
EOF
  rc=0; reach "$BIN" t "$TD/notpure.anb" "$TD/poison.anb" >/dev/null || rc=$?
  ck "non-pure is MALFORMED" 3 "$rc"

  if [[ "$fails" -gt 0 ]]; then echo "GUARD_REACHABILITY SELFTEST: FAIL ($fails)"; exit 1; fi
  echo "GUARD_REACHABILITY SELFTEST: PASS"
  exit 0
fi

if [[ $# -lt 3 ]]; then
  echo "usage: $0 NAME PURE.anb POISON.anb  |  $0 --self-test" >&2
  exit 64
fi
reach "$(resolve_bin)" "$1" "$2" "$3"
