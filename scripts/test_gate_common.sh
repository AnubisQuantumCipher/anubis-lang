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

cat >"$TMP/rust_zero.log" <<'EOF'
running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out; finished in 0.00s
EOF
set +e
rust_zero_output="$(assert_rust_tests_exercised "$TMP/rust_zero.log" "zero filter" 2>&1)"
rc=$?
set -e
[[ "$rc" -eq 1 && "$rust_zero_output" == *"matched zero tests"* ]] && ok=1 || ok=0
record rust_zero_filter "$ok" "rc=$rc output=$rust_zero_output"

cat >"$TMP/rust_multi.log" <<'EOF'
running 2 tests
test a ... ok
test b ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s

running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

running 1 test
test c ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s
EOF
set +e
assert_rust_tests_exercised "$TMP/rust_multi.log" "multi harness"
rc=$?
set -e
[[ "$rc" -eq 0 && "$GATE_RUST_TESTS_PASSED" -eq 3 ]] && ok=1 || ok=0
record rust_multi_harness "$ok" "rc=$rc passed=${GATE_RUST_TESTS_PASSED:-unset}"

cat >"$TMP/rust_malformed_positive.log" <<'EOF'
running 2 tests
test a ... ok
test b ... ok
test result: ok. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
EOF
set +e
rust_malformed_output="$(assert_rust_tests_exercised "$TMP/rust_malformed_positive.log" "malformed positive" 2>&1)"
rc=$?
set -e
[[ "$rc" -ne 0 && "$rust_malformed_output" == *"libtest summary"* ]] && ok=1 || ok=0
record rust_rejects_positive_with_failures "$ok" "rc=$rc output=$rust_malformed_output"

cat >"$TMP/rust_truncated_positive.log" <<'EOF'
running 2 tests
test result: ok. 2 passed;
EOF
set +e
rust_truncated_output="$(assert_rust_tests_exercised "$TMP/rust_truncated_positive.log" "truncated positive" 2>&1)"
rc=$?
set -e
[[ "$rc" -eq 2 && "$rust_truncated_output" == *"unparseable libtest summary"* ]] && ok=1 || ok=0
record rust_rejects_truncated_positive "$ok" "rc=$rc output=$rust_truncated_output"

cat >"$TMP/rust_missing_duration.log" <<'EOF'
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
EOF
set +e
rust_missing_duration_output="$(assert_rust_tests_exercised "$TMP/rust_missing_duration.log" "missing duration" 2>&1)"
rc=$?
set -e
[[ "$rc" -eq 2 && "$rust_missing_duration_output" == *"unparseable libtest summary"* ]] && ok=1 || ok=0
record rust_rejects_missing_duration "$ok" "rc=$rc output=$rust_missing_duration_output"

cat >"$TMP/rust_malformed_duration.log" <<'EOF'
running 2 tests
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in malformed
EOF
set +e
rust_malformed_duration_output="$(assert_rust_tests_exercised "$TMP/rust_malformed_duration.log" "malformed duration" 2>&1)"
rc=$?
set -e
[[ "$rc" -eq 2 && "$rust_malformed_duration_output" == *"unparseable libtest summary"* ]] && ok=1 || ok=0
record rust_rejects_malformed_duration "$ok" "rc=$rc output=$rust_malformed_duration_output"

printf 'anubis test: 0/0 passed\n' >"$TMP/anubis_tests_zero.log"
set +e
assert_anubis_tests_exercised "$TMP/anubis_tests_zero.log" "dx fixtures" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -ne 0 ]] && ok=1 || ok=0
record anubis_tests_reject_zero "$ok" "rc=$rc"

printf 'anubis test: 1/2 passed\n' >"$TMP/anubis_tests_partial.log"
set +e
assert_anubis_tests_exercised "$TMP/anubis_tests_partial.log" "dx fixtures" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -ne 0 ]] && ok=1 || ok=0
record anubis_tests_reject_partial "$ok" "rc=$rc"

printf 'anubis test: 2/2 passed\n' >"$TMP/anubis_tests_green.log"
set +e
assert_anubis_tests_exercised "$TMP/anubis_tests_green.log" "dx fixtures" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 0 && "$GATE_ANUBIS_TESTS_TOTAL" -eq 2 ]] && ok=1 || ok=0
record anubis_tests_counted "$ok" "rc=$rc total=${GATE_ANUBIS_TESTS_TOTAL:-unset}"

cat >"$TMP/anubis_tests_mixed_malformed.log" <<'EOF'
anubis test: 2/2 passed
anubis test: malformed summary
EOF
set +e
assert_anubis_tests_exercised "$TMP/anubis_tests_mixed_malformed.log" "dx fixtures" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 2 ]] && ok=1 || ok=0
record anubis_tests_reject_mixed_malformed "$ok" "rc=$rc"

printf '4x\n' >"$TMP/malformed_floor"
set +e
assert_floor "fixtures" 5 "$TMP/malformed_floor" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 2 && "$(<"$TMP/malformed_floor")" == "4x" ]] && ok=1 || ok=0
record floor_rejects_malformed_content "$ok" "rc=$rc floor=$(<"$TMP/malformed_floor")"

printf '08\n' >"$TMP/noncanonical_floor"
set +e
assert_floor "fixtures" 8 "$TMP/noncanonical_floor" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 2 && "$(<"$TMP/noncanonical_floor")" == "08" ]] && ok=1 || ok=0
record floor_rejects_noncanonical_content "$ok" "rc=$rc floor=$(<"$TMP/noncanonical_floor")"

printf '4\n' >"$TMP/concurrent_floor"
set +e
assert_floor "fixtures" 5 "$TMP/concurrent_floor" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 0 && "$(<"$TMP/concurrent_floor")" == "4" ]] && ok=1 || ok=0
record floor_check_is_read_only "$ok" "rc=$rc floor=$(<"$TMP/concurrent_floor")"

mkdir "$TMP/concurrent_floor.lock"
set +e
ANUBIS_GATE_UPDATE_FLOORS=1 assert_floor "fixtures" 5 "$TMP/concurrent_floor" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 2 && "$(<"$TMP/concurrent_floor")" == "4" ]] && ok=1 || ok=0
record floor_rejects_concurrent_update "$ok" "rc=$rc floor=$(<"$TMP/concurrent_floor")"

printf '4\n' >"$TMP/atomic_floor"
mv() { return 1; }
set +e
ANUBIS_GATE_UPDATE_FLOORS=1 assert_floor "fixtures" 5 "$TMP/atomic_floor" >/dev/null 2>&1
rc=$?
set -e
unset -f mv
[[ "$rc" -eq 2 && "$(<"$TMP/atomic_floor")" == "4" ]] && ok=1 || ok=0
record floor_update_is_atomic "$ok" "rc=$rc floor=$(<"$TMP/atomic_floor")"

set +e
assert_floor "fixtures" 5 "$TMP/missing_floor" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 2 && ! -e "$TMP/missing_floor" ]] && ok=1 || ok=0
record floor_missing_read_only_rejected "$ok" "rc=$rc"

set +e
ANUBIS_GATE_UPDATE_FLOORS=1 assert_floor "fixtures" 5 "$TMP/missing_floor" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 0 && "$(<"$TMP/missing_floor")" == "5" ]] && ok=1 || ok=0
record floor_maintenance_initialises "$ok" "rc=$rc floor=$(<"$TMP/missing_floor")"

: >"$TMP/rust_missing.log"
set +e
rust_missing_output="$(assert_rust_tests_exercised "$TMP/rust_missing.log" "missing summary" 2>&1)"
rc=$?
set -e
[[ "$rc" -eq 2 && "$rust_missing_output" == *"no libtest summaries"* ]] && ok=1 || ok=0
record rust_missing_summary "$ok" "rc=$rc output=$rust_missing_output"

mkdir -p "$TMP/output_clean/.gate-lock"
set +e
assert_clean_output_dir "$TMP/output_clean" ".gate-lock" "clean output" >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 0 ]] && ok=1 || ok=0
record output_dir_accepts_only_lock "$ok" "rc=$rc"

mkdir -p "$TMP/output_stale/.gate-lock"
printf '{}\n' >"$TMP/output_stale/stale.json"
set +e
stale_output="$(assert_clean_output_dir "$TMP/output_stale" ".gate-lock" "stale output" 2>&1)"
rc=$?
set -e
[[ "$rc" -eq 1 && "$stale_output" == *"stale artifacts"* ]] && ok=1 || ok=0
record output_dir_rejects_stale "$ok" "rc=$rc output=$stale_output"

mkdir -p "$TMP/output_unreadable/.gate-lock"
find() { return 9; }
set +e
unreadable_output="$(assert_clean_output_dir "$TMP/output_unreadable" ".gate-lock" "unreadable output" 2>&1)"
rc=$?
set -e
unset -f find
[[ "$rc" -eq 2 && "$unreadable_output" == *"could not enumerate"* ]] && ok=1 || ok=0
record output_dir_rejects_enumeration_failure "$ok" "rc=$rc output=$unreadable_output"

broad_was_set="${RISC0_SKIP_BUILD_KERNELS+x}"
broad_saved="${RISC0_SKIP_BUILD_KERNELS:-}"
targeted_was_set="${ANUBIS_SKIP_RISC0_METAL+x}"
targeted_saved="${ANUBIS_SKIP_RISC0_METAL:-}"
runtime_was_set="${R0_DISABLE_METAL+x}"
runtime_saved="${R0_DISABLE_METAL:-}"
unset RISC0_SKIP_BUILD_KERNELS ANUBIS_SKIP_RISC0_METAL R0_DISABLE_METAL
gate_configure_audit_profile_environment hosted
[[ -z "${RISC0_SKIP_BUILD_KERNELS+x}" \
  && "${ANUBIS_SKIP_RISC0_METAL:-}" == 1 \
  && "${R0_DISABLE_METAL:-}" == 1 ]] && ok=1 || ok=0
record hosted_profile_keeps_cpu_and_skips_metal "$ok" \
  "broad=${RISC0_SKIP_BUILD_KERNELS+x} targeted=${ANUBIS_SKIP_RISC0_METAL:-unset} runtime=${R0_DISABLE_METAL:-unset}"

RISC0_SKIP_BUILD_KERNELS=1
ANUBIS_SKIP_RISC0_METAL=0
R0_DISABLE_METAL=0
gate_configure_audit_profile_environment hosted
[[ -z "${RISC0_SKIP_BUILD_KERNELS+x}" \
  && "$ANUBIS_SKIP_RISC0_METAL" == 1 \
  && "$R0_DISABLE_METAL" == 1 ]] && ok=1 || ok=0
record hosted_profile_overrides_conflicting_metal_settings "$ok" \
  "broad=${RISC0_SKIP_BUILD_KERNELS+x} targeted=$ANUBIS_SKIP_RISC0_METAL runtime=$R0_DISABLE_METAL"

export RISC0_SKIP_BUILD_KERNELS=1 ANUBIS_SKIP_RISC0_METAL=1 R0_DISABLE_METAL=1
gate_configure_audit_profile_environment full
[[ -z "${RISC0_SKIP_BUILD_KERNELS+x}" \
  && -z "${ANUBIS_SKIP_RISC0_METAL+x}" \
  && -z "${R0_DISABLE_METAL+x}" ]] && ok=1 || ok=0
record full_profile_clears_all_metal_bypasses "$ok" \
  "broad=${RISC0_SKIP_BUILD_KERNELS+x} targeted=${ANUBIS_SKIP_RISC0_METAL+x} runtime=${R0_DISABLE_METAL+x}"

set +e
gate_configure_audit_profile_environment unsupported >/dev/null 2>&1
rc=$?
set -e
[[ "$rc" -eq 2 ]] && ok=1 || ok=0
record audit_profile_rejects_unknown "$ok" "rc=$rc"

if [[ -n "$broad_was_set" ]]; then
  export RISC0_SKIP_BUILD_KERNELS="$broad_saved"
else
  unset RISC0_SKIP_BUILD_KERNELS
fi
if [[ -n "$targeted_was_set" ]]; then
  export ANUBIS_SKIP_RISC0_METAL="$targeted_saved"
else
  unset ANUBIS_SKIP_RISC0_METAL
fi
if [[ -n "$runtime_was_set" ]]; then
  export R0_DISABLE_METAL="$runtime_saved"
else
  unset R0_DISABLE_METAL
fi

printf 'GATE_COMMON_SELFTEST: %s (pass=%d fail=%d artifact=%s)\n' \
  "$([[ "$fail" -eq 0 ]] && echo PASS || echo FAIL)" "$pass" "$fail" "$TMP"
[[ "$fail" -eq 0 ]]
