#!/usr/bin/env bash
# Adversarial microbench for scripts/lib/gate_common.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

TMP="$(mktemp -d "${TMPDIR:-/tmp}/anubis-gate-common.XXXXXX")"
pass=0
fail=0

record() {
  local name="$1" ok="$2" detail="$3"
  if [[ "$ok" == "1" ]]; then
    pass=$((pass + 1))
    printf 'PASS %-24s %s\n' "$name" "$detail"
  else
    fail=$((fail + 1))
    printf 'FAIL %-24s %s\n' "$name" "$detail"
  fi
}

printf '// EXPECT: PASS\nfn main() {}\n' >"$TMP/valid_accepts.anb"
printf '// EXPECT: PASS\r\nfn main() {}\r\n' >"$TMP/crlf_accepts.anb"
printf 'fn main() { print("EXPECT: PASS"); }\n' >"$TMP/body_string_accepts.anb"
printf 'fn main() {}\n// EXPECT: PASS\n' >"$TMP/body_comment_accepts.anb"
printf '// EXPECT: PASS\n// EXPECT: FAIL\nfn main() {}\n' >"$TMP/conflict_accepts.anb"
printf '// EXPECT: PASS\n// EXPECT: PASS\nfn main() {}\n' >"$TMP/duplicate_accepts.anb"
printf '// EXPECT: PASS\nfn main() {}\n' >"$TMP/symlink_target.anb"
ln -s "$TMP/symlink_target.anb" "$TMP/symlink_accepts.anb"
: >"$TMP/empty_accepts.anb"
printf '// EXPECT: PASS\n' >"$TMP/unreadable_accepts.anb"
chmod 000 "$TMP/unreadable_accepts.anb"
mkdir "$TMP/dirs_only" "$TMP/dirs_only/child"

set +e
parse_expectation "$TMP/valid_accepts.anb" both_rejects_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == *"both _rejects and _accepts"* ]] \
  && ok=1 || ok=0
record both_name "$ok" "rc=$rc malformed=$GATE_MALFORMED"

set +e
parse_expectation "$TMP/crlf_accepts.anb" crlf_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 0 && "$GATE_EXPECT" == "PASS" ]] && ok=1 || ok=0
record crlf "$ok" "rc=$rc expect=${GATE_EXPECT:-unset}"

set +e
parse_expectation "$TMP/body_string_accepts.anb" body_string_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == "missing EXPECT: header" ]] && ok=1 || ok=0
record body_string "$ok" "rc=$rc malformed=$GATE_MALFORMED"

set +e
parse_expectation "$TMP/body_comment_accepts.anb" body_comment_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == "missing EXPECT: header" ]] && ok=1 || ok=0
record body_comment "$ok" "rc=$rc malformed=$GATE_MALFORMED"

set +e
parse_expectation "$TMP/conflict_accepts.anb" conflict_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == "multiple conflicting EXPECT: headers" ]] \
  && ok=1 || ok=0
record conflicting_headers "$ok" "rc=$rc malformed=$GATE_MALFORMED"

set +e
parse_expectation "$TMP/duplicate_accepts.anb" duplicate_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == "multiple EXPECT: headers" ]] && ok=1 || ok=0
record duplicate_headers "$ok" "rc=$rc malformed=$GATE_MALFORMED"

set +e
parse_expectation "$TMP/symlink_accepts.anb" symlink_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == "fixture is a symbolic link" ]] && ok=1 || ok=0
record symlink "$ok" "rc=$rc malformed=$GATE_MALFORMED"

set +e
parse_expectation "$TMP/empty_accepts.anb" empty_accepts accept_reject
rc=$?
set -e
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == "fixture is empty" ]] && ok=1 || ok=0
record empty_file "$ok" "rc=$rc malformed=$GATE_MALFORMED"

set +e
parse_expectation "$TMP/unreadable_accepts.anb" unreadable_accepts accept_reject
rc=$?
set -e
chmod 600 "$TMP/unreadable_accepts.anb"
[[ "$rc" -eq 1 && "$GATE_MALFORMED" == "fixture is unreadable" ]] && ok=1 || ok=0
record unreadable "$ok" "rc=$rc malformed=$GATE_MALFORMED"

shopt -s nullglob
dir_matches=( "$TMP/dirs_only"/*.anb )
shopt -u nullglob
set +e
corpus_output="$(require_nonempty_corpus "${#dir_matches[@]}" "$TMP/dirs_only/*.anb" 2>&1)"
rc=$?
set -e
[[ "$rc" -eq 1 && "$corpus_output" == *"EMPTY CORPUS"* ]] && ok=1 || ok=0
record dirs_only "$ok" "rc=$rc output=$corpus_output"

set +e
finalize 3 2 0 0
rc=$?
set -e
[[ "$rc" -eq 2 && "$GATE_FINAL_STATUS" == "INCOMPLETE" \
  && "$GATE_FINAL_REASON" == *"do not sum"* ]] && ok=1 || ok=0
record counter_mismatch "$ok" \
  "rc=$rc status=$GATE_FINAL_STATUS reason=$GATE_FINAL_REASON"

printf 'GATE_COMMON_SELFTEST: %s (pass=%d fail=%d artifact=%s)\n' \
  "$([[ "$fail" -eq 0 ]] && echo PASS || echo FAIL)" "$pass" "$fail" "$TMP"
[[ "$fail" -eq 0 ]]
