#!/usr/bin/env bash
# Focused regression tests for scripts/phase_metrics.sh --append-ledger.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/anubis-phase-metrics-test.XXXXXX")" || exit 2
trap 'chmod -R u+w "$TMP" 2>/dev/null || true; rm -rf "$TMP"' EXIT
pass=0
fail=0

record() {
  local name="$1" ok="$2" detail="$3"
  if [[ "$ok" == 1 ]]; then
    pass=$((pass + 1))
    printf 'PASS %-32s %s\n' "$name" "$detail"
  else
    fail=$((fail + 1))
    printf 'FAIL %-32s %s\n' "$name" "$detail"
  fi
}

mode_of() {
  stat -f '%Lp' "$1" 2>/dev/null || stat -c '%a' "$1" 2>/dev/null
}

sha_of() {
  shasum -a 256 "$1" | cut -d' ' -f1
}

lock_path() {
  local ledger="$1" key
  ledger="$(cd "$(dirname "$ledger")" && pwd -L)/$(basename "$ledger")"
  key="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.argv[1].encode()).hexdigest()[:24])' "$ledger")"
  printf '%s/anubis-phase-metrics-locks-%s/%s.lock\n' \
    "${TMPDIR:-/tmp}" "$(id -u)" "$key"
}

fixture() {
  local name="$1" base="$TMP/$1"
  mkdir -p "$base/scripts" "$base/docs/evidence" \
    "$base/compiler/src/middle" "$base/compiler/src/frontend"
  cp "$ROOT/scripts/phase_metrics.sh" "$base/scripts/phase_metrics.sh"
  chmod 0755 "$base/scripts/phase_metrics.sh"
  cp "$ROOT/compiler/src/middle/mod.rs" "$base/compiler/src/middle/mod.rs"
  cp "$ROOT/compiler/src/middle/capability.rs" "$base/compiler/src/middle/capability.rs"
  cp "$ROOT/compiler/src/middle/effects.rs" "$base/compiler/src/middle/effects.rs"
  cp "$ROOT/compiler/src/frontend/mod.rs" "$base/compiler/src/frontend/mod.rs"
  printf '%s\n' "$base"
}

# A normal first append must create one durable observation and report it only after creation.
base="$(fixture normal)"
out="$(bash "$base/scripts/phase_metrics.sh" --append-ledger 2>&1)"
rc=$?
ledger="$base/docs/evidence/PHASE_METRICS_LEDGER.md"
headings=$(grep -c '^## ' "$ledger" 2>/dev/null || true)
[[ $rc -eq 0 && -f "$ledger" && $headings -eq 1 && "$out" == *"PHASE_METRICS_LEDGER: APPENDED"* ]] \
  && ok=1 || ok=0
record normal_append "$ok" "rc=$rc headings=$headings"

families="$(printf '%s\n' "$out" | awk '$1 == "walker" && $2 == "families" {print $3; exit}')"
[[ $rc -eq 0 && "$families" == 4 ]] && ok=1 || ok=0
record thin_shared_adapters_not_families "$ok" "rc=$rc families=${families:-missing} expected=4"

# A label wrapper that regains AST structure is independent again and must be counted.
base="$(fixture structural_wrapper)"
python3 - "$base/compiler/src/middle/mod.rs" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
s = p.read_text()
start = s.index('fn walk_block_taint(')
call = s.index('    walk_block_labels(', start)
s = s[:call] + '    if matches!(stmts.first(), Some(Stmt::Break)) {}\n' + s[call:]
p.write_text(s)
PY
out="$(bash "$base/scripts/phase_metrics.sh" 2>&1)"
rc=$?
families="$(printf '%s\n' "$out" | awk '$1 == "walker" && $2 == "families" {print $3; exit}')"
[[ $rc -eq 0 && "$families" == 5 ]] && ok=1 || ok=0
record structural_wrapper_is_family "$ok" "rc=$rc families=${families:-missing} expected=5"

# An existing directory at the ledger path is invalid and must never produce APPENDED.
base="$(fixture directory)"
ledger="$base/docs/evidence/PHASE_METRICS_LEDGER.md"
mkdir "$ledger"
set +e
out="$(bash "$base/scripts/phase_metrics.sh" --append-ledger 2>&1)"
rc=$?
set -u
entries=$(find "$ledger" -mindepth 1 -maxdepth 1 -type f | wc -l | tr -d ' ')
[[ $rc -ne 0 && -d "$ledger" && $entries -eq 0 && "$out" != *"PHASE_METRICS_LEDGER: APPENDED"* ]] \
  && ok=1 || ok=0
record directory_is_rejected "$ok" "rc=$rc files_in_directory=$entries"

# A held append lock must fail closed without changing the ledger.
base="$(fixture locked)"
ledger="$base/docs/evidence/PHASE_METRICS_LEDGER.md"
printf '# ledger\n' >"$ledger"
lock="$(lock_path "$ledger")"
mkdir -p "$(dirname "$lock")"
mkdir "$lock"
before="$(sha_of "$ledger")"
set +e
out="$(bash "$base/scripts/phase_metrics.sh" --append-ledger 2>&1)"
rc=$?
set -u
after="$(sha_of "$ledger")"
[[ $rc -ne 0 && "$before" == "$after" && "$out" != *"PHASE_METRICS_LEDGER: APPENDED"* ]] \
  && ok=1 || ok=0
rmdir "$lock" 2>/dev/null || true
record concurrent_lock_is_rejected "$ok" "rc=$rc unchanged=$([[ "$before" == "$after" ]] && echo yes || echo no)"

# Read-only means frozen: reject instead of replacing it with a writable file.
base="$(fixture readonly)"
ledger="$base/docs/evidence/PHASE_METRICS_LEDGER.md"
printf '# ledger\n' >"$ledger"
chmod 0444 "$ledger"
before="$(sha_of "$ledger")"
set +e
out="$(bash "$base/scripts/phase_metrics.sh" --append-ledger 2>&1)"
rc=$?
set -u
after="$(sha_of "$ledger")"
mode="$(mode_of "$ledger")"
[[ $rc -ne 0 && "$before" == "$after" && "$mode" == 444 && "$out" != *"PHASE_METRICS_LEDGER: APPENDED"* ]] \
  && ok=1 || ok=0
record readonly_ledger_is_frozen "$ok" "rc=$rc mode=$mode unchanged=$([[ "$before" == "$after" ]] && echo yes || echo no)"

# A writable non-default mode is part of the artifact and must survive replacement.
base="$(fixture mode)"
ledger="$base/docs/evidence/PHASE_METRICS_LEDGER.md"
printf '# ledger\n' >"$ledger"
chmod 0640 "$ledger"
out="$(bash "$base/scripts/phase_metrics.sh" --append-ledger 2>&1)"
rc=$?
mode="$(mode_of "$ledger")"
[[ $rc -eq 0 && "$mode" == 640 && "$out" == *"PHASE_METRICS_LEDGER: APPENDED"* ]] \
  && ok=1 || ok=0
record writable_mode_is_preserved "$ok" "rc=$rc mode=$mode"

# A RED measurement is still evidence: append it, retain rc=2, and print APPENDED truthfully.
base="$(fixture red)"
rm "$base/compiler/src/frontend/mod.rs"
set +e
out="$(bash "$base/scripts/phase_metrics.sh" --append-ledger 2>&1)"
rc=$?
set -u
ledger="$base/docs/evidence/PHASE_METRICS_LEDGER.md"
headings=$(grep -c '^## ' "$ledger" 2>/dev/null || true)
[[ $rc -eq 2 && -f "$ledger" && $headings -eq 1 \
   && "$out" == *"PHASE_METRICS_LEDGER: APPENDED"* \
   && "$out" == *"FATAL: missing compiler/src/frontend/mod.rs"* ]] \
  && ok=1 || ok=0
record red_measurement_is_recorded "$ok" "rc=$rc headings=$headings"

# Append machinery must not alter the dirty count captured inside its own observation.
base="$(fixture self_contamination)"
git -C "$base" init -q
git -C "$base" add scripts compiler
git -C "$base" -c user.name=AnubisTest -c user.email=test@example.invalid \
  commit -qm 'fixture baseline'
before_dirty="$(git -C "$base" status --porcelain | wc -l | tr -d ' ')"
out="$(bash "$base/scripts/phase_metrics.sh" --append-ledger 2>&1)"
rc=$?
recorded_dirty="$(printf '%s\n' "$out" | awk '/^dirty[[:space:]]*:/ {print $3; exit}')"
[[ $rc -eq 0 && $before_dirty -eq 0 && "$recorded_dirty" == 0 ]] && ok=1 || ok=0
record append_does_not_measure_itself "$ok" \
  "rc=$rc baseline_dirty=$before_dirty recorded_dirty=${recorded_dirty:-missing}"

# Real concurrent attempts may serialize or one may refuse, but APPENDED successes must equal entries.
base="$(fixture concurrent)"
ledger="$base/docs/evidence/PHASE_METRICS_LEDGER.md"
bash "$base/scripts/phase_metrics.sh" --append-ledger >"$base/a.out" 2>&1 & a=$!
bash "$base/scripts/phase_metrics.sh" --append-ledger >"$base/b.out" 2>&1 & b=$!
set +e
wait "$a"; a_rc=$?
wait "$b"; b_rc=$?
set -u
headings=$(grep -c '^## ' "$ledger" 2>/dev/null || true)
appended=$(grep -h -c 'PHASE_METRICS_LEDGER: APPENDED' "$base/a.out" "$base/b.out" | awk '{s+=$1} END {print s+0}')
successes=0
[[ $a_rc -eq 0 ]] && successes=$((successes + 1))
[[ $b_rc -eq 0 ]] && successes=$((successes + 1))
[[ $headings -eq $successes && $appended -eq $successes && $successes -ge 1 ]] \
  && ok=1 || ok=0
record concurrent_observations_conserved "$ok" \
  "rcs=$a_rc,$b_rc headings=$headings appended=$appended successes=$successes"

printf 'PHASE_METRICS_LEDGER_TESTS: %s passed, %s failed\n' "$pass" "$fail"
[[ $fail -eq 0 ]]
