#!/usr/bin/env bash
set -uo pipefail
# fixture_preflight.sh — decide whether an ACCEPT is even CAPABLE of being a finding.
#
#   Usage: fixture_preflight.sh NAME LEAK.anb DIRECT.anb
#          fixture_preflight.sh --self-test
#   Env:   ANUBIS_BIN (default: the published pin)
#
#   exit 0  YES_OPEN   direct twin REJECTS, leak ACCEPTS, nothing on the path authorized it
#   exit 2  CLOSED     both reject — the mechanism reaches this shape
#   exit 3  MALFORMED  the fixture cannot reject no matter what the compiler does
#
# WHY THIS EXISTS
#
# Three times in one session a fixture was reported as an open false-accept when no rejection
# was available to it. Every one had the same shape: THE TEST GRANTED THE THING IT WAS TRYING
# TO CATCH. `w06b_reassign_to_leak` declared `uses(fs.write)` on both the forwarder and `main`,
# so the write was authorized and ACCEPT was correct; it was carried as an open mechanism for
# four rounds and survived a compiler fix that had in fact already closed it.
#
# An ACCEPT is evidence of a defect only once a rejection is known to exist. That is what this
# checks, and it is cheap enough to run before reporting any finding.
#
# NOTE ON `set -e`: this script deliberately does NOT use `set -e`. The version this was
# promoted from did, and it therefore ABORTED with an undefined rc=1 the instant the direct
# twin rejected — the normal case — so it never once printed YES_OPEN. A preflight harness that
# dies before its own verdict is the exact defect class it was written to prevent. `check`
# returning non-zero is DATA here, not failure, and the script is structured so that it can
# never again be mistaken for one.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

resolve_bin() {
  if [[ -n "${ANUBIS_BIN:-}" ]]; then echo "$ANUBIS_BIN"; return 0; fi
  local pin; pin="$(bash scripts/publish_pin.sh --current 2>/dev/null)"
  if [[ -n "$pin" && -x "$pin" ]]; then echo "$pin"; return 0; fi
  echo "./target/release/anubis"
}

# Does any function OTHER than the effectful one declare a capability?
#
# If it does, the effect is authorized and no verdict but ACCEPT is available. The scan is
# deliberately over `fn NAME` boundaries rather than a bare `grep uses(` so that a `uses(...)`
# on the leak function itself — which is REQUIRED for the fixture to mean anything — is not
# mistaken for an authorization on the path.
uses_on_path() {
  local file="$1"
  awk '
    /^[[:space:]]*fn[[:space:]]+/ {
      # The effectful helper is allowed (indeed required) to declare its own capability.
      is_leak = ($0 ~ /fn[[:space:]]+(leak|leak_[A-Za-z0-9_]*|[A-Za-z0-9_]*_leak)[[:space:]]*\(/)
    }
    !is_leak && /uses[[:space:]]*\(/ { found = 1 }
    END { exit found ? 0 : 1 }
  ' "$file"
}

verdict_of() {
  local bin="$1" file="$2" rc=0
  "$bin" check "$file" >/dev/null 2>&1 || rc=$?
  echo "$rc"
}

preflight() {
  local bin="$1" name="$2" leak="$3" direct="$4"

  for f in "$leak" "$direct"; do
    if [[ ! -f "$f" ]]; then echo "MALFORMED missing_file:$f"; return 3; fi
  done

  if uses_on_path "$leak"; then
    echo "MALFORMED uses_on_path — a non-leak function declares a capability, so the effect is authorized and ACCEPT is the only available verdict"
    return 3
  fi

  local dec lec
  dec="$(verdict_of "$bin" "$direct")"
  lec="$(verdict_of "$bin" "$leak")"

  if [[ "$dec" -eq 0 ]]; then
    echo "MALFORMED direct_ACCEPT — the simplest form of this effect is not rejected either, so the instrument cannot express the finding"
    return 3
  fi
  if [[ "$lec" -ne 0 ]]; then
    echo "CLOSED both_reject (direct rc=$dec, leak rc=$lec)"
    return 2
  fi
  echo "YES_OPEN direct_REJECT(rc=$dec) leak_ACCEPT(rc=0)"
  return 0
}

if [[ "${1:-}" == "--self-test" ]]; then
  BIN="$(resolve_bin)"
  TD="$(mktemp -d)"; trap 'rm -rf "$TD"' EXIT
  fails=0
  ck() { # ck LABEL EXPECTED_RC ACTUAL_RC
    if [[ "$2" != "$3" ]]; then echo "  SELFTEST FAIL: $1 expected rc=$2 got rc=$3"; fails=$((fails+1));
    else echo "  ok: $1 (rc=$3)"; fi
  }

  # 1. YES_OPEN — the case the promoted-from version could never print.
  cat > "$TD/direct.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(f: any) { f("/tmp/pf.txt", "x"); }
fn main() { app(leak); }
EOF
  cat > "$TD/leak.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(xs: any) { let ys = xs; ys[0]("/tmp/pf.txt", "x"); }
fn main() { app([leak]); }
EOF
  rc=0; preflight "$BIN" t "$TD/leak.anb" "$TD/direct.anb" >/dev/null || rc=$?
  # Either verdict is legitimate depending on whether the shape is closed; rc=1 is NOT.
  if [[ "$rc" -eq 1 ]]; then
    echo "  SELFTEST FAIL: aborted with the undefined rc=1 (the set -e regression)"; fails=$((fails+1))
  else echo "  ok: reaches a defined verdict on a rejecting direct twin (rc=$rc)"; fi

  # 2. MALFORMED — w06b verbatim, the fixture that started this.
  cat > "$TD/w06b.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(xs: any) uses(fs.write) { xs = [leak]; xs[0]("/tmp/pf.txt", "x"); }
fn main() uses(fs.write) { app([leak]); }
EOF
  rc=0; preflight "$BIN" t "$TD/w06b.anb" "$TD/direct.anb" >/dev/null || rc=$?
  ck "w06b is caught as MALFORMED" 3 "$rc"

  # 3. MALFORMED — a direct twin that does not reject means the instrument is blind here.
  cat > "$TD/pure_direct.anb" <<'EOF'
fn pure_fn(p: string, x: string) { }
fn app(f: any) { f("/tmp/pf.txt", "x"); }
fn main() { app(pure_fn); }
EOF
  rc=0; preflight "$BIN" t "$TD/leak.anb" "$TD/pure_direct.anb" >/dev/null || rc=$?
  ck "non-rejecting direct twin is caught" 3 "$rc"

  # 4. A missing file must not be silently graded.
  rc=0; preflight "$BIN" t "$TD/nope.anb" "$TD/direct.anb" >/dev/null || rc=$?
  ck "missing fixture is caught" 3 "$rc"

  if [[ "$fails" -gt 0 ]]; then echo "FIXTURE_PREFLIGHT SELFTEST: FAIL ($fails)"; exit 1; fi
  echo "FIXTURE_PREFLIGHT SELFTEST: PASS"
  exit 0
fi

if [[ $# -lt 3 ]]; then
  echo "usage: $0 NAME LEAK.anb DIRECT.anb   |   $0 --self-test" >&2
  exit 64
fi

preflight "$(resolve_bin)" "$1" "$2" "$3"
