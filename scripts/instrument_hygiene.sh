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
# R43 prospective addition:
#   5. Every gate that reports a numeric score/count must assert a floor
#      (require_nonempty_corpus / assert_tested / FLOOR file) — docs drift lost 29%
#      coverage and stayed green until a ratchet existed.
#   6. Security EXPECT PASS/FAIL corpus size must not silently shrink.
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
else
  if grep -q 'PIN DOES NOT MATCH\|pin matches\|PIN MATCH' /tmp/ih_pin.out; then
    ok "publish_pin --verify prints an explicit mismatch/match (no silent pass)"
  else
    bad "publish_pin --verify produced no explicit verdict"
  fi
fi

# --- 2. pin binary exists and is executable ---
PIN="$(bash scripts/publish_pin.sh --current 2>/dev/null || true)"
if [[ -n "$PIN" && -x "$PIN" ]]; then
  ok "current pin is executable: $PIN"
else
  bad "current pin missing or not executable"
fi

# --- 3. $? discipline: capture exit BEFORE other commands ---
rc=0
false || rc=$?
_=$(basename /tmp/x 2>/dev/null)
if [[ "$rc" -eq 1 ]]; then
  ok "\$? captured before basename (rc=$rc)"
else
  bad "\$? lost after intervening command (rc=$rc)"
fi

# --- 4. fixture_preflight must not use set -e ---
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
  tail -5 /tmp/ih_pf.out
fi

# --- 6. guard_reachability --self-test ---
if [[ -x scripts/guard_reachability.sh ]]; then
  if ANUBIS_BIN="${ANUBIS_BIN:-$PIN}" bash scripts/guard_reachability.sh --self-test >/tmp/ih_gr.out 2>&1; then
    ok "guard_reachability --self-test PASS"
  else
    bad "guard_reachability --self-test FAIL"
  fi
else
  bad "guard_reachability.sh missing"
fi

# --- 7. preflight defined exit codes: w06b-style -> MALFORMED 3, not abort 1 ---
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
  bad "preflight unexpected rc=$rc on w06b-style"
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

# --- 9. Prospective: count-reporting gates must have a floor ---
# A gate "reports a count" only if it prints a shrinkable aggregate (Overall N/M,
# pass=/fail= summary, total++, stamps_checked, files scanned). Bare PASS/FAIL tokens
# and hollow `Overall: $overall` are NOT count reporters (adversary R47/R48 split).
# `assert_tested` / `require_nonempty_corpus` are NOT floors (count==0 only).
mkdir -p scratchpad/fleet_20260726/adversary/r43
python3 - <<'PY' > /tmp/ih_gate_floors_run.out
import re
from pathlib import Path

def reports_shrinkable_count(t: str) -> bool:
    if re.search(r"stamps_checked|STAMPS_CHECKED", t):
        return True
    if re.search(r"Overall:.*\(\$pass/\$total\)|Overall:.*\(\$passed/\$total\)|Overall: \$verdict \(\$pass/\$total\)", t):
        return True
    if re.search(r"echo \"Overall:.*\(\$pass/\$total\)", t):
        return True
    if "total=$((total + 1))" in t or "total=$((total+1))" in t:
        return True
    # pass/fail summaries (echo, note, tee)
    if re.search(r"pass=\$pass fail=\$fail", t) and re.search(r"pass=\$\(\(pass", t):
        return True
    if re.search(r"#fixtures\[@\]", t) and re.search(r"Overall:|passed|total", t):
        return True
    if re.search(r'\bn=\$\(.*find|\bn=\$\(.*wc', t) and re.search(r"\$n\b", t):
        return True
    # total=0 ... total=$((total+1)) style even without Overall
    if re.search(r"\btotal=0\b", t) and re.search(r"total=\$\(\(total", t):
        return True
    if re.search(r"total=\$\(\(pass\+fail\)\)|total=\$\(\(agree", t):
        return True
    if re.search(r'jq -r [\'"]\.total', t):
        return True
    if re.search(r"\bn=0\b", t) and re.search(r"n=\$\(\(n\+1\)\)|n=\$\(\(n \+ 1\)\)", t):
        return True
    return False

paths = sorted(Path("scripts").glob("run_*gate*.sh")) + sorted(Path("scripts").glob("run_*fixtures*.sh"))
seen = set()
rows = []
bait = []
for p in paths:
    if p.name in seen:
        continue
    seen.add(p.name)
    t = p.read_text(errors="replace")
    generous = bool(re.search(
        r"Overall:|stamps_checked|fixtures=\(|passed=|fail=|total=|over [0-9]+ fixtures|pass=\$\(|green\)",
        t,
    ))
    reports = reports_shrinkable_count(t)
    has_floor = bool(re.search(r"assert_floor|_floor\b|ratchet", t))
    if reports:
        rows.append((p.name, has_floor))
    elif generous:
        bait.append(p.name)
missing = [a for a, b in rows if not b]
out = Path("scratchpad/fleet_20260726/adversary/r43")
out.mkdir(parents=True, exist_ok=True)
(out / "gate_floor_missing.list").write_text("\n".join(missing) + ("\n" if missing else ""))
(out / "gate_floor_false_positive.list").write_text("\n".join(bait) + ("\n" if bait else ""))
(out / "gate_floor_inventory.tsv").write_text(
    "script\thas_floor\n" + "\n".join(f"{a}\t{int(b)}" for a, b in rows) + "\n"
)
print(
    f"count_reporting_gates={len(rows)} "
    f"with_floor={sum(1 for _, b in rows if b)} "
    f"missing_floor={len(missing)} "
    f"false_positive_bait={len(bait)}"
)
for m in missing:
    print(f"MISSING_FLOOR {m}")
for b in bait:
    print(f"FALSE_POSITIVE {b}")
raise SystemExit(0 if not missing else 2)
PY
floor_rc=$?
cat /tmp/ih_gate_floors_run.out
if [[ "$floor_rc" -eq 0 ]]; then
  ok "every genuine count-reporting gate has assert_floor / floor / ratchet"
else
  miss=$(grep -c '^MISSING_FLOOR' /tmp/ih_gate_floors_run.out || true)
  bad "genuine count-reporting gates missing a floor: $miss (scratchpad/fleet_20260726/adversary/r43/gate_floor_missing.list)"
fi
bait_n=$(grep -c '^FALSE_POSITIVE' /tmp/ih_gate_floors_run.out || true)
if [[ "${bait_n:-0}" -gt 0 ]]; then
  ok "detector excluded $bait_n PASS-token/hollow-Overall bait (see gate_floor_false_positive.list)"
fi

# --- 10. Docs drift coverage floor file must exist and be numeric >0 ---
if [[ -f docs/.docs_drift_coverage_floor ]]; then
  fl=$(tr -dc '0-9' < docs/.docs_drift_coverage_floor)
  if [[ -n "$fl" && "$fl" -gt 0 ]]; then
    ok "docs drift coverage floor present and >0 ($fl)"
  else
    bad "docs drift coverage floor unparseable or zero"
  fi
else
  bad "docs/.docs_drift_coverage_floor missing"
fi

# --- 11. Security EXPECT PASS/FAIL corpus size floor (ratchet) ---
PASS_N=$(rg -l '^// EXPECT: PASS' examples/security --glob '*.anb' 2>/dev/null | wc -l | tr -d ' ')
FAIL_N=$(rg -l '^// EXPECT: FAIL' examples/security --glob '*.anb' 2>/dev/null | wc -l | tr -d ' ')
SEC_FLOOR_FILE="examples/security/.corpus_expect_floor"
if [[ -f "$SEC_FLOOR_FILE" ]]; then
  FLOOR_PASS=$(awk '{print $1}' "$SEC_FLOOR_FILE" | tr -dc '0-9')
  FLOOR_FAIL=$(awk '{print $2}' "$SEC_FLOOR_FILE" | tr -dc '0-9')
  FLOOR_PASS=${FLOOR_PASS:-0}
  FLOOR_FAIL=${FLOOR_FAIL:-0}
  if [[ "$PASS_N" -lt "$FLOOR_PASS" || "$FAIL_N" -lt "$FLOOR_FAIL" ]]; then
    bad "security corpus shrank: PASS $PASS_N (floor $FLOOR_PASS) FAIL $FAIL_N (floor $FLOOR_FAIL)"
  else
    ok "security corpus floors held (PASS $PASS_N>=$FLOOR_PASS FAIL $FAIL_N>=$FLOOR_FAIL)"
    if [[ "$PASS_N" -gt "$FLOOR_PASS" || "$FAIL_N" -gt "$FLOOR_FAIL" ]]; then
      echo "$PASS_N $FAIL_N" > "$SEC_FLOOR_FILE"
      ok "security corpus floor ratchet raised to PASS=$PASS_N FAIL=$FAIL_N"
    fi
  fi
else
  echo "$PASS_N $FAIL_N" > "$SEC_FLOOR_FILE"
  ok "security corpus floor initialised PASS=$PASS_N FAIL=$FAIL_N"
fi

# --- 12. Pure proof-correspondence gate must be explicitly exempt from pin invocation. ---
proof_block="$(awk '
  /^run_gate proof_correspondence[[:space:]]*\\/ { in_block=1 }
  in_block { print }
  in_block && /run_proof_correspondence_gate[.]sh/ { exit }
' scripts/run_seal_checklist.sh)"
if [[ "$proof_block" == *"--no-pin-use"* && "$proof_block" == *"run_proof_correspondence_gate.sh"* ]]; then
  ok "proof_correspondence is explicitly source-only (--no-pin-use)"
else
  bad "proof_correspondence lacks its explicit source-only --no-pin-use contract"
fi

# --- 13. Suite freshness is a separate seal gate over a run-local ledger. ---
# Do not execute it here: the standalone gate immediately after this one owns the
# complete 19/23-row pre-freshness roster and promotes nothing until final validation.
if [[ -x scripts/gate_run_freshness.sh ]] \
   && grep -Fq 'run_gate gate_run_freshness' scripts/run_seal_checklist.sh; then
  ok "gate_run_freshness is separately wired into the seal"
else
  bad "gate_run_freshness missing or not separately wired into the seal"
fi

if [[ "$fails" -gt 0 ]]; then
  echo "INSTRUMENT_HYGIENE: FAIL ($fails)"
  exit 1
fi
echo "INSTRUMENT_HYGIENE: PASS"
exit 0
