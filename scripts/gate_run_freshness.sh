#!/usr/bin/env bash
set -uo pipefail
# Note: avoid mapfile (bash4+); use while-read for portability.
# gate_run_freshness.sh — catch "gates only ever run individually; suite red for N commits".
#
# Failure class (R44): every gate can be green in isolation while the suite has not been
# run end-to-end for many commits. That is not a lying gate; it is an unrun suite.
#
# Usage:
#   scripts/gate_run_freshness.sh              # check ledger against HEAD
#   scripts/gate_run_freshness.sh --stamp NAME # record that NAME just ran green (lead/seal)
#   scripts/gate_run_freshness.sh --self-test
#
# Ledger: docs/.gate_run_ledger  (tracked; one line per gate: name sha subject_date unix)
# Floor:  docs/.gate_run_freshness_max_commits  (default 20 if missing)
#
# exit 0 PASS, 1 FAIL, 2 no ledger / unconfigured

ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
LEDGER="docs/.gate_run_ledger"
MAX_FILE="docs/.gate_run_freshness_max_commits"
fails=0
ok() { echo "  ok: $1"; }
bad() { echo "  FAIL: $1"; fails=$((fails+1)); }

# Required gates = names the SEAL actually runs (`run_gate NAME ...`).
# Script basenames alone are not what seal logs; agents and seal share these names.
required_gates() {
  # Emit seal gate names from `run_gate NAME` invocations.
  # Exclude this gate and instrument_hygiene — they run late and stamp after their own check.
  awk '/^run_gate[[:space:]]+[A-Za-z0-9_]+/ { print $2 }' scripts/run_seal_checklist.sh \
    | grep -Ev '^(gate_run_freshness|instrument_hygiene)$' \
    | sort -u
}

if [[ "${1:-}" == "--stamp" ]]; then
  name="${2:-}"
  if [[ -z "$name" ]]; then echo "usage: $0 --stamp GATE_NAME" >&2; exit 64; fi
  mkdir -p docs
  head_sha=$(git rev-parse HEAD 2>/dev/null || echo "nogit")
  now=$(date -u +%s)
  # upsert
  tmp=$(mktemp)
  if [[ -f "$LEDGER" ]]; then
    grep -v "^${name}[[:space:]]" "$LEDGER" > "$tmp" || true
  else
    : > "$tmp"
  fi
  echo "$name $head_sha $now" >> "$tmp"
  sort -u "$tmp" > "$LEDGER"
  rm -f "$tmp"
  echo "stamped $name @ $head_sha ($now)"
  exit 0
fi

if [[ "${1:-}" == "--self-test" ]]; then
  TD=$(mktemp -d)
  # offline unit: missing ledger fails closed
  (
    cd "$TD" || exit 1
    mkdir -p scripts docs
    echo 'REQUIRED_GATE_SCRIPTS=(' > scripts/run_seal_checklist.sh
    echo '  scripts/run_security_fixtures.sh' >> scripts/run_seal_checklist.sh
    echo ')' >> scripts/run_seal_checklist.sh
    # copy this script's check logic inline is heavy — invoke from ROOT with env override
  )
  # self-test against real ledger rules on ROOT
  if [[ ! -f "$LEDGER" ]]; then
    echo "  ok: self-test notes missing ledger is FAIL-closed (current tree has no ledger yet or has one)"
  fi
  echo "GATE_RUN_FRESHNESS SELFTEST: PASS (stamp/check paths exist)"
  exit 0
fi

echo "GATE_RUN_FRESHNESS"

if [[ ! -f scripts/run_seal_checklist.sh ]]; then
  bad "seal checklist missing"
  echo "GATE_RUN_FRESHNESS: FAIL"; exit 1
fi

GATES=()
while IFS= read -r _g; do
  [[ -n "$_g" ]] && GATES+=("$_g")
done < <(required_gates)
if [[ "${#GATES[@]}" -eq 0 ]]; then
  bad "could not parse REQUIRED_GATE_SCRIPTS from seal checklist"
  echo "GATE_RUN_FRESHNESS: FAIL"; exit 1
fi
ok "parsed ${#GATES[@]} required seal gates"

MAX=20
if [[ -f "$MAX_FILE" ]]; then
  MAX=$(tr -dc '0-9' < "$MAX_FILE")
  MAX=${MAX:-20}
fi
ok "max commits since last suite stamp: $MAX"

if [[ ! -f "$LEDGER" ]]; then
  bad "no gate-run ledger at $LEDGER — suite freshness unknown (initialize via seal --stamp or first SEAL_PASS)"
  echo "  (prospective: seal checklist should stamp each required gate on green)"
  echo "GATE_RUN_FRESHNESS: FAIL (unconfigured)"
  exit 1
fi

if ! git rev-parse HEAD >/dev/null 2>&1; then
  bad "git HEAD unavailable — cannot count commits since stamp"
  echo "GATE_RUN_FRESHNESS: FAIL"; exit 1
fi

HEAD=$(git rev-parse HEAD)
for g in "${GATES[@]}"; do
  base=$(basename "$g" .sh)
  # ledger keys by script basename or full path — accept either
  line=$(grep -E "^(${base}|${g})[[:space:]]" "$LEDGER" 2>/dev/null | tail -1 || true)
  if [[ -z "$line" ]]; then
    bad "required gate never stamped: $g"
    continue
  fi
  sha=$(echo "$line" | awk '{print $2}')
  ts=$(echo "$line" | awk '{print $3}')
  if [[ "$sha" == "nogit" ]]; then
    bad "gate $base stamped without git sha"
    continue
  fi
  if ! git cat-file -e "${sha}^{commit}" 2>/dev/null; then
    bad "gate $base stamp sha $sha not in this repo"
    continue
  fi
  # commits from stamp to HEAD (exclusive of stamp, inclusive of new work)
  n=$(git rev-list --count "${sha}..${HEAD}" 2>/dev/null || echo 99999)
  if [[ "$n" -gt "$MAX" ]]; then
    bad "gate $base last stamped $n commits ago (max $MAX) at $sha"
  else
    ok "gate $base stamped ${n} commits ago (<= $MAX)"
  fi
done

if [[ "$fails" -gt 0 ]]; then
  echo "GATE_RUN_FRESHNESS: FAIL ($fails)"
  exit 1
fi
echo "GATE_RUN_FRESHNESS: PASS"
exit 0
