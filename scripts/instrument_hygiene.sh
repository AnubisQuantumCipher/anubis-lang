#!/usr/bin/env bash
set -uo pipefail
# instrument_hygiene.sh — catch silent tool failures of the kind that misled this session.
#
# Four silent failures this session (lead R42 brief):
#   1. $? after command substitution / basename / pipeline  (exit code of wrong command)
#   2. set -e aborting a harness when check REJECT is data
#   3. measuring a binary that does not match the pin / tree (stale scoring)
#   4. uses(…) on non-leak path misread as a finding when it authorizes the effect
#
# This is a meta-check: it does not grade fixtures. It grades the *tools* we grade with.
# exit 0 PASS, 1 FAIL.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
fails=0
ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; fails=$((fails+1)); }

echo "INSTRUMENT_HYGIENE"

# --- 1. pin verify must be runnable and reported honestly ---
if bash scripts/publish_pin.sh --verify >/tmp/ih_pin.out 2>&1; then
  ok "publish_pin --verify PASS (pin matches tree)"
  pin_ok=1
else
  # Not a hard fail of the instrument: drift is allowed if agents pin-measure.
  # Hard fail only if --verify itself is silent or exit-ambiguous.
  if grep -q 'PIN DOES NOT MATCH\|pin matches\|PIN MATCH' /tmp/ih_pin.out; then
    ok "publish_pin --verify prints an explicit mismatch/match (no silent pass)"
    pin_ok=0
  else
    bad "publish_pin --verify produced no explicit verdict"
    pin_ok=0
  fi
fi

# --- 2. pin binary exists and is executable ---
PIN="$(bash scripts/publish_pin.sh --current 2>/dev/null || true)"
if [[ -n "$PIN" && -x "$PIN" ]]; then
  ok "current pin is executable: $PIN"
else
  bad "current pin missing or not executable"
fi

# --- 3. $? discipline demo: capture exit BEFORE other commands ---
# (documents the anti-pattern; fails if a wrapper loses the code)
rc=0
false || rc=$?
_=$(basename /tmp/x 2>/dev/null)
if [[ "$rc" -eq 1 ]]; then
  ok "\$? captured before basename (rc=$rc)"
else
  bad "\$? lost after intervening command (rc=$rc)"
fi

# --- 4. fixture_preflight must not use set -e (grep the source) ---
if grep -qE '^set -e' scripts/fixture_preflight.sh 2>/dev/null; then
  bad "fixture_preflight.sh still has set -e (will abort on direct REJECT)"
else
  ok "fixture_preflight.sh has no set -e"
fi

# --- 5. fixture_preflight --self-test must PASS ---
if ANUBIS_BIN="${ANUBIS_BIN:-$PIN}" bash scripts/fixture_preflight.sh --self-test >/tmp/ih_pf.out 2>&1; then
  ok "fixture_preflight --self-test PASS"
else
  bad "fixture_preflight --self-test FAIL"
  cat /tmp/ih_pf.out | tail -5
fi

# --- 6. guard_reachability --self-test if present ---
if [[ -x scripts/guard_reachability.sh ]]; then
  if ANUBIS_BIN="${ANUBIS_BIN:-$PIN}" bash scripts/guard_reachability.sh --self-test >/tmp/ih_gr.out 2>&1; then
    ok "guard_reachability --self-test PASS"
  else
    bad "guard_reachability --self-test FAIL"
  fi
else
  ok "guard_reachability not present (skip)"
fi

# --- 7. preflight defined exit codes only: 0/2/3, never bare 1 from harness abort ---
# smoke: w06b-style MALFORMED
TD=$(mktemp -d)
cat > "$TD/d.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(f: any) { f("/tmp/x","y"); }
fn main() { app(leak); }
EOF
cat > "$TD/w.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(xs: any) uses(fs.write) { xs[0]("/tmp/x","y"); }
fn main() uses(fs.write) { app([leak]); }
EOF
rc=0
ANUBIS_BIN="${ANUBIS_BIN:-$PIN}" bash scripts/fixture_preflight.sh t "$TD/w.anb" "$TD/d.anb" >/tmp/ih_w06.out 2>&1 || rc=$?
if [[ "$rc" -eq 3 ]]; then
  ok "w06b-style yields MALFORMED rc=3 (not abort rc=1)"
elif [[ "$rc" -eq 1 ]]; then
  bad "preflight aborted with rc=1 on authorized uses path"
else
  bad "preflight unexpected rc=$rc on w06b-style (out=$(cat /tmp/ih_w06.out))"
fi
rm -rf "$TD"

# --- 8. uses_on_path boundary: leak-only uses must NOT trip MALFORMED ---
TD=$(mktemp -d)
cat > "$TD/d.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(f: any) { f("/tmp/x","y"); }
fn main() { app(leak); }
EOF
cat > "$TD/l.anb" <<'EOF'
fn leak(p: string, x: string) uses(fs.write) { write_file(p, x); }
fn app(xs: any) { xs[0]("/tmp/x","y"); }
fn main() { app([leak]); }
EOF
rc=0
ANUBIS_BIN="${ANUBIS_BIN:-$PIN}" bash scripts/fixture_preflight.sh t "$TD/l.anb" "$TD/d.anb" >/tmp/ih_uop.out 2>&1 || rc=$?
if grep -q 'uses_on_path' /tmp/ih_uop.out; then
  bad "uses_on_path false-positive on leak-only uses"
else
  ok "uses_on_path boundary: leak-only uses not MALFORMED (rc=$rc)"
fi
rm -rf "$TD"

if [[ "$fails" -gt 0 ]]; then
  echo "INSTRUMENT_HYGIENE: FAIL ($fails)"
  exit 1
fi
echo "INSTRUMENT_HYGIENE: PASS"
exit 0
