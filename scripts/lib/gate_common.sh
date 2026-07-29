#!/usr/bin/env bash
# Shared fail-closed primitives for fixture gates. Callers retain their own
# domain-specific execution and reporting; these functions only enforce the
# invariants that must not vary between harnesses.

# parse_expectation FILE BASENAME [NAME_POLICY]
# Result variables: GATE_EXPECT, GATE_MALFORMED, GATE_EXPECT_COUNT.
# NAME_POLICY "accept_reject" binds *_accepts/*_rejects to their headers.
# NAME_POLICY "should_fail_closed" binds names containing should_fail_closed to FAIL.
#
# Only an exact `// EXPECT: PASS|FAIL` line in the leading comment/blank header block
# is authoritative. Program-body strings and comments are not fixture metadata.
parse_expectation() {
  GATE_EXPECT=""
  GATE_MALFORMED=""
  GATE_EXPECT_COUNT=0

  if [[ $# -lt 2 ]]; then
    GATE_MALFORMED="parse_expectation requires FILE and BASENAME"
    return 2
  fi

  local file="$1" base="$2" name_policy="${3:-none}"
  if [[ -L "$file" ]]; then
    GATE_MALFORMED="fixture is a symbolic link"
    return 1
  fi
  if [[ ! -f "$file" ]]; then
    GATE_MALFORMED="fixture is not a regular file"
    return 1
  fi
  if [[ ! -r "$file" ]]; then
    GATE_MALFORMED="fixture is unreadable"
    return 1
  fi
  if [[ ! -s "$file" ]]; then
    GATE_MALFORMED="fixture is empty"
    return 1
  fi

  local parsed conflict
  if ! parsed="$(awk '
    BEGIN { in_header=1; count=0; first=""; conflict=0 }
    {
      sub(/\r$/, "", $0)
      if (in_header) {
        if ($0 ~ /^[[:space:]]*$/) next
        if ($0 ~ /^[[:space:]]*\/\//) {
          if ($0 ~ /^[[:space:]]*\/\/[[:space:]]*EXPECT:[[:space:]]*(PASS|FAIL)[[:space:]]*$/) {
            value=$0
            sub(/^[[:space:]]*\/\/[[:space:]]*EXPECT:[[:space:]]*/, "", value)
            sub(/[[:space:]]*$/, "", value)
            count++
            if (first == "") first=value
            else if (value != first) conflict=1
          }
          next
        }
        in_header=0
      }
    }
    END { printf "%d|%s|%d\n", count, first, conflict }
  ' "$file" 2>/dev/null)"; then
    GATE_MALFORMED="could not parse fixture header"
    return 1
  fi
  IFS='|' read -r GATE_EXPECT_COUNT GATE_EXPECT conflict <<<"$parsed"

  if [[ "$GATE_EXPECT_COUNT" -eq 0 ]]; then
    GATE_MALFORMED="missing EXPECT: header"
    return 1
  fi
  if [[ "$GATE_EXPECT_COUNT" -gt 1 ]]; then
    if [[ "$conflict" -eq 1 ]]; then
      GATE_MALFORMED="multiple conflicting EXPECT: headers"
    else
      GATE_MALFORMED="multiple EXPECT: headers"
    fi
    return 1
  fi

  case "$name_policy" in
    none)
      ;;
    accept_reject)
      if [[ "$base" == *"_rejects"* && "$base" == *"_accepts"* ]]; then
        GATE_MALFORMED="name contains both _rejects and _accepts"
      else
        case "$base" in
          *_rejects)
            [[ "$GATE_EXPECT" == "FAIL" ]] \
              || GATE_MALFORMED="name says _rejects but header says EXPECT: $GATE_EXPECT"
            ;;
          *_accepts)
            [[ "$GATE_EXPECT" == "PASS" ]] \
              || GATE_MALFORMED="name says _accepts but header says EXPECT: $GATE_EXPECT"
            ;;
        esac
      fi
      ;;
    should_fail_closed)
      if [[ "$base" == *"should_fail_closed"* && "$GATE_EXPECT" != "FAIL" ]]; then
        GATE_MALFORMED="name says should_fail_closed but header says EXPECT: $GATE_EXPECT"
      fi
      ;;
    *)
      GATE_MALFORMED="unknown expectation name policy: $name_policy"
      ;;
  esac

  [[ -z "$GATE_MALFORMED" ]]
}

# score_fixture EXPECTED ACTUAL
# Result variable: GATE_FIXTURE_STATUS.
score_fixture() {
  GATE_FIXTURE_STATUS="INVALID"
  if [[ $# -ne 2 ]]; then
    return 2
  fi
  local expected="$1" actual="$2"
  GATE_FIXTURE_STATUS="FAIL"
  case "$expected" in PASS|FAIL) ;; *) return 2 ;; esac
  case "$actual" in PASS|FAIL) ;; *) return 2 ;; esac
  if [[ "$expected" == "$actual" ]]; then
    GATE_FIXTURE_STATUS="PASS"
    return 0
  fi
  return 1
}

# require_nonempty_corpus COUNT DESCRIPTION
require_nonempty_corpus() {
  GATE_CORPUS_ERROR=""
  if [[ $# -ne 2 ]]; then
    GATE_CORPUS_ERROR="require_nonempty_corpus requires COUNT and DESCRIPTION"
    echo "INVALID CORPUS COUNT: $GATE_CORPUS_ERROR - refusing to report PASS" >&2
    return 2
  fi
  local count="$1" description="$2"
  if [[ ! "$count" =~ ^(0|[1-9][0-9]*)$ ]]; then
    GATE_CORPUS_ERROR="count is not a canonical non-negative integer: $count"
    echo "INVALID CORPUS COUNT: $GATE_CORPUS_ERROR - refusing to report PASS" >&2
    return 2
  fi
  if [[ "$count" == "0" ]]; then
    GATE_CORPUS_ERROR="no fixtures matched $description"
    echo "EMPTY CORPUS: no fixtures matched $description - refusing to report PASS" >&2
    return 1
  fi
  return 0
}

# assert_tested COUNT DESCRIPTION [COUNT2 DESCRIPTION2 ...]
#
# Coverage is part of a verdict, not decoration. A gate whose headline prints a coverage number it
# never checks cannot distinguish "measured everything and it agreed" from "measured nothing".
#
# This is not hypothetical. `run_docs_drift_gate.sh` gated only on its FAILURE counter and printed
# `Overall: PASS (0 stamps checked, 0 drift)` with exit 0 against an empty scan root — demonstrated
# by counterexample 2026-07-28. Its scanner also skipped missing owned docs with a bare `continue`,
# so an ordinary rename produced the same vacuous green, and two of its fifteen declared owned docs
# were absent and silently skipped on the live tree.
#
# `require_nonempty_corpus` covers the fixture-corpus shape. This covers everything else a gate
# counts: stamps, guards, scenarios, probes, assertions — any number a reader would take as
# evidence that work happened.
#
# Result variable: GATE_COVERAGE_ERROR.
assert_tested() {
  GATE_COVERAGE_ERROR=""
  if [[ $# -lt 2 || $(( $# % 2 )) -ne 0 ]]; then
    GATE_COVERAGE_ERROR="assert_tested requires COUNT DESCRIPTION pairs"
    echo "INVALID COVERAGE ASSERTION: $GATE_COVERAGE_ERROR - refusing to report PASS" >&2
    return 2
  fi
  while [[ $# -gt 0 ]]; do
    local count="$1" description="$2"
    shift 2
    if [[ ! "$count" =~ ^(0|[1-9][0-9]*)$ ]]; then
      GATE_COVERAGE_ERROR="coverage counter is not a canonical non-negative integer: $description=$count"
      echo "INVALID COVERAGE COUNT: $GATE_COVERAGE_ERROR - refusing to report PASS" >&2
      return 2
    fi
    if [[ "$count" == "0" ]]; then
      GATE_COVERAGE_ERROR="tested nothing: $description=0"
      echo "VACUOUS GATE: $description is 0 - the gate tested nothing, refusing to report PASS" >&2
      return 1
    fi
  done
  return 0
}

# assert_clean_output_dir DIRECTORY EXCLUDED_ENTRY DESCRIPTION
#
# Require a gate-owned output directory to contain only its already-acquired
# lock. Enumeration failure is infrastructure failure, never evidence of an
# empty directory.
assert_clean_output_dir() {
  GATE_OUTPUT_DIR_ERROR=""
  if [[ $# -ne 3 ]]; then
    GATE_OUTPUT_DIR_ERROR="assert_clean_output_dir requires DIRECTORY EXCLUDED_ENTRY DESCRIPTION"
    echo "INVALID OUTPUT ASSERTION: $GATE_OUTPUT_DIR_ERROR" >&2
    return 2
  fi
  local directory="$1" excluded_entry="$2" description="$3" entries
  if [[ ! -d "$directory" ]]; then
    GATE_OUTPUT_DIR_ERROR="missing output directory for $description: $directory"
    echo "INVALID OUTPUT DIRECTORY: $GATE_OUTPUT_DIR_ERROR" >&2
    return 2
  fi
  if ! entries="$(find "$directory" -mindepth 1 -maxdepth 1 \
    ! -name "$excluded_entry" -print -quit)"; then
    GATE_OUTPUT_DIR_ERROR="could not enumerate output directory for $description: $directory"
    echo "INVALID OUTPUT DIRECTORY: $GATE_OUTPUT_DIR_ERROR" >&2
    return 2
  fi
  if [[ -n "$entries" ]]; then
    GATE_OUTPUT_DIR_ERROR="stale artifacts in output directory for $description: $directory"
    echo "STALE OUTPUT DIRECTORY: $GATE_OUTPUT_DIR_ERROR" >&2
    return 1
  fi
  return 0
}

# assert_rust_tests_exercised LOG_FILE DESCRIPTION
#
# A filtered `cargo test` exits 0 when its filter matches zero tests. Matching only
# `test result: ok` therefore turns a renamed test into a vacuous PASS. Cargo may
# run several libtest harnesses for one invocation, so sum every parsed `N passed`
# summary and require the aggregate to be non-zero.
#
# Result variables: GATE_RUST_TESTS_PASSED and GATE_RUST_TESTS_ERROR.
assert_rust_tests_exercised() {
  GATE_RUST_TESTS_PASSED=0
  GATE_RUST_TESTS_ERROR=""
  if [[ $# -ne 2 ]]; then
    GATE_RUST_TESTS_ERROR="assert_rust_tests_exercised requires LOG_FILE DESCRIPTION"
    echo "INVALID RUST TEST ASSERTION: $GATE_RUST_TESTS_ERROR" >&2
    return 2
  fi
  local log="$1" description="$2"
  if [[ ! -f "$log" ]]; then
    GATE_RUST_TESTS_ERROR="missing Rust test log: $log"
    echo "INVALID RUST TEST LOG: $GATE_RUST_TESTS_ERROR" >&2
    return 2
  fi

  local summaries parsed_count malformed_summaries
  summaries="$(grep -E '^test result: ' "$log" 2>/dev/null || true)"
  if [[ -z "$summaries" ]]; then
    GATE_RUST_TESTS_ERROR="no libtest summaries for $description"
    echo "INVALID RUST TEST LOG: $GATE_RUST_TESTS_ERROR" >&2
    return 2
  fi
  if printf '%s\n' "$summaries" | grep -qE '^test result: (FAILED|failed)'; then
    GATE_RUST_TESTS_ERROR="a libtest harness failed for $description"
    echo "RUST TEST FAILURE: $GATE_RUST_TESTS_ERROR" >&2
    return 1
  fi

  malformed_summaries="$(printf '%s\n' "$summaries" | grep -Ev \
    '^test result: ok\. [0-9]+ passed; 0 failed; [0-9]+ ignored; [0-9]+ measured; [0-9]+ filtered out; finished in [0-9]+(\.[0-9]+)?s$' \
    || true)"
  if [[ -n "$malformed_summaries" ]]; then
    GATE_RUST_TESTS_ERROR="unparseable libtest summary for $description"
    echo "INVALID RUST TEST LOG: $GATE_RUST_TESTS_ERROR" >&2
    return 2
  fi

  parsed_count="$(printf '%s\n' "$summaries" \
    | sed -nE 's/^test result: ok\. ([0-9]+) passed; 0 failed; [0-9]+ ignored; [0-9]+ measured; [0-9]+ filtered out; finished in [0-9]+(\.[0-9]+)?s$/\1/p' \
    | awk '{ total += $1; rows += 1 } END { printf "%d %d", total, rows }')"
  GATE_RUST_TESTS_PASSED="${parsed_count%% *}"
  local parsed_rows="${parsed_count##* }"
  local summary_rows
  summary_rows="$(printf '%s\n' "$summaries" | wc -l | tr -d ' ')"
  if [[ "$parsed_rows" -ne "$summary_rows" ]]; then
    GATE_RUST_TESTS_ERROR="unparseable libtest summary for $description"
    echo "INVALID RUST TEST LOG: $GATE_RUST_TESTS_ERROR" >&2
    return 2
  fi
  if [[ "$GATE_RUST_TESTS_PASSED" -eq 0 ]]; then
    GATE_RUST_TESTS_ERROR="filter matched zero tests for $description"
    echo "VACUOUS RUST TEST FILTER: $GATE_RUST_TESTS_ERROR - refusing to report PASS" >&2
    return 1
  fi
  return 0
}

# assert_anubis_tests_exercised LOG_FILE DESCRIPTION
# Parse the CLI's declared `anubis test: PASSED/TOTAL passed` verdict and refuse
# zero-work, partial, duplicate, or malformed summaries.
assert_anubis_tests_exercised() {
  GATE_ANUBIS_TESTS_TOTAL=0
  GATE_ANUBIS_TESTS_PASSED=0
  GATE_ANUBIS_TESTS_ERROR=""
  if [[ $# -ne 2 ]]; then
    GATE_ANUBIS_TESTS_ERROR="assert_anubis_tests_exercised requires LOG_FILE DESCRIPTION"
    return 2
  fi
  local log="$1" description="$2" summary_lines summaries row passed total
  if [[ ! -f "$log" ]]; then
    GATE_ANUBIS_TESTS_ERROR="missing Anubis test log: $log"
    return 2
  fi
  summary_lines="$(grep -E '^anubis test:' "$log" 2>/dev/null || true)"
  summaries="$(printf '%s\n' "$summary_lines" \
    | grep -E '^anubis test: [0-9]+/[0-9]+ passed$' 2>/dev/null || true)"
  if [[ -z "$summary_lines" \
    || "$(printf '%s\n' "$summary_lines" | wc -l | tr -d ' ')" -ne 1 \
    || "$(printf '%s\n' "$summaries" | wc -l | tr -d ' ')" -ne 1 ]]; then
    GATE_ANUBIS_TESTS_ERROR="expected exactly one parseable Anubis test summary for $description"
    return 2
  fi
  row="$(printf '%s\n' "$summaries" | sed -E 's/^anubis test: ([0-9]+)\/([0-9]+) passed$/\1 \2/')"
  passed="${row%% *}"
  total="${row##* }"
  GATE_ANUBIS_TESTS_PASSED="$passed"
  GATE_ANUBIS_TESTS_TOTAL="$total"
  if [[ "$total" -eq 0 ]]; then
    GATE_ANUBIS_TESTS_ERROR="Anubis test matched zero fixtures for $description"
    return 1
  fi
  if [[ "$passed" -ne "$total" ]]; then
    GATE_ANUBIS_TESTS_ERROR="Anubis test incomplete for $description: passed=$passed total=$total"
    return 1
  fi
  return 0
}

# finalize TOTAL PASSED FAILED [INCOMPLETE]
# Result variables: GATE_FINAL_STATUS (PASS, FAIL, INCOMPLETE, or INVALID)
# and GATE_FINAL_REASON.
finalize() {
  GATE_FINAL_STATUS="INVALID"
  GATE_FINAL_REASON=""
  if [[ $# -lt 3 || $# -gt 4 ]]; then
    GATE_FINAL_REASON="finalize requires TOTAL PASSED FAILED [INCOMPLETE]"
    return 2
  fi
  local total="$1" passed="$2" failed="$3" incomplete="${4:-0}"
  local value
  for value in "$total" "$passed" "$failed" "$incomplete"; do
    if [[ ! "$value" =~ ^(0|[1-9][0-9]*)$ ]]; then
      GATE_FINAL_REASON="counter is not a canonical non-negative integer: $value"
      return 2
    fi
  done

  require_nonempty_corpus "$total" "the configured corpus" || return 1
  if [[ $((passed + failed + incomplete)) -ne "$total" ]]; then
    GATE_FINAL_STATUS="INCOMPLETE"
    GATE_FINAL_REASON="counters do not sum to total: total=$total passed=$passed failed=$failed incomplete=$incomplete"
    return 2
  fi
  if [[ "$failed" -gt 0 ]]; then
    GATE_FINAL_STATUS="FAIL"
    GATE_FINAL_REASON="$failed fixture(s) failed"
    return 1
  fi
  if [[ "$incomplete" -gt 0 ]]; then
    GATE_FINAL_STATUS="INCOMPLETE"
    GATE_FINAL_REASON="$incomplete fixture(s) incomplete"
    return 2
  fi
  GATE_FINAL_STATUS="PASS"
  return 0
}

# assert_floor NAME COUNT FLOOR_FILE
#
# A coverage RATCHET. `assert_tested` catches a gate that tested NOTHING; it cannot catch one that
# quietly tests LESS, and the difference is not academic — the docs-drift gate silently went from
# 42 stamps to 30 (a 29% loss) and reported `PASS (30 stamps checked, 0 drift)` with nothing to
# indicate anything had changed. Every exemption that caused it was justified on review, which is
# precisely the problem: the justified case and the careless case produce identical output.
#
# Ordinary verification is read-only. Raising or initialising a floor requires the explicit
# maintenance switch `ANUBIS_GATE_UPDATE_FLOORS=1`; otherwise a test run could silently mutate the
# source tree and certify its own newly-written baseline. Lowering still means editing the tracked
# file in a visible commit.
#
# Returns 0 if COUNT >= FLOOR, 1 otherwise. In maintenance mode it atomically raises the floor.
assert_floor() {
  GATE_FLOOR_ERROR=""
  if [[ $# -ne 3 ]]; then
    GATE_FLOOR_ERROR="assert_floor requires NAME COUNT FLOOR_FILE"
    echo "INVALID FLOOR ASSERTION: $GATE_FLOOR_ERROR - refusing to report PASS" >&2
    return 2
  fi
  local name="$1" count="$2" file="$3"
  if [[ ! "$count" =~ ^(0|[1-9][0-9]*)$ ]]; then
    GATE_FLOOR_ERROR="$name count is not a canonical non-negative integer: '$count'"
    echo "INVALID FLOOR COUNT: $GATE_FLOOR_ERROR - refusing to report PASS" >&2
    return 2
  fi
  local floor tmp lock update="${ANUBIS_GATE_UPDATE_FLOORS:-0}"
  tmp="${file}.tmp.$$"
  lock="${file}.lock"
  if [[ "$update" != 0 && "$update" != 1 ]]; then
    GATE_FLOOR_ERROR="ANUBIS_GATE_UPDATE_FLOORS must be 0 or 1"
    echo "INVALID FLOOR MODE: $GATE_FLOOR_ERROR - refusing to grade" >&2
    return 2
  fi
  if [[ ! -f "$file" ]]; then
    if [[ "$update" != 1 ]]; then
      GATE_FLOOR_ERROR="coverage floor is missing: $file (set ANUBIS_GATE_UPDATE_FLOORS=1 only for reviewed maintenance)"
      echo "MISSING FLOOR FILE: $GATE_FLOOR_ERROR - refusing to grade" >&2
      return 2
    fi
    if ! mkdir "$lock" 2>/dev/null; then
      GATE_FLOOR_ERROR="coverage floor is already being updated: $file"
      echo "CONCURRENT FLOOR UPDATE: $GATE_FLOOR_ERROR - refusing to grade" >&2
      return 2
    fi
    if ! printf '%s\n' "$count" > "$tmp" || ! mv "$tmp" "$file"; then
      rm -f "$tmp"
      rmdir "$lock" 2>/dev/null || true
      GATE_FLOOR_ERROR="could not atomically initialise floor file: $file"
      echo "INVALID FLOOR FILE: $GATE_FLOOR_ERROR - refusing to grade" >&2
      return 2
    fi
    if ! rmdir "$lock" 2>/dev/null; then
      GATE_FLOOR_ERROR="could not release coverage floor lock: $lock"
      echo "INVALID FLOOR LOCK: $GATE_FLOOR_ERROR" >&2
      return 2
    fi
    echo "coverage floor initialised: $name=$count ($file)"
    return 0
  fi
  floor="$(<"$file")"
  if [[ ! "$floor" =~ ^(0|[1-9][0-9]*)$ ]]; then
    GATE_FLOOR_ERROR="floor file is unparseable: $file"
    echo "INVALID FLOOR FILE: $GATE_FLOOR_ERROR - refusing to grade" >&2
    return 2
  fi
  if [[ "$count" -lt "$floor" ]]; then
    GATE_FLOOR_ERROR="coverage fell: $name=$count, floor is $floor"
    echo "COVERAGE REGRESSION: $GATE_FLOOR_ERROR" >&2
    echo "  Something stopped being checked. If the loss is correct, lower $file in the same commit and say why." >&2
    return 1
  fi
  if [[ "$count" -gt "$floor" ]]; then
    if [[ "$update" != 1 ]]; then
      echo "coverage exceeds tracked floor (read-only): $name $floor -> $count"
      return 0
    fi
    if ! mkdir "$lock" 2>/dev/null; then
      GATE_FLOOR_ERROR="coverage floor is already being updated: $file"
      echo "CONCURRENT FLOOR UPDATE: $GATE_FLOOR_ERROR - refusing to grade" >&2
      return 2
    fi
    # Re-read under the update lock; another maintenance process may have raised it.
    floor="$(<"$file")"
    if [[ ! "$floor" =~ ^(0|[1-9][0-9]*)$ || "$count" -lt "$floor" ]]; then
      rmdir "$lock" 2>/dev/null || true
      GATE_FLOOR_ERROR="floor changed incompatibly while acquiring update lock: $file"
      echo "CONCURRENT FLOOR UPDATE: $GATE_FLOOR_ERROR - refusing to grade" >&2
      return 2
    fi
    if ! printf '%s\n' "$count" > "$tmp" || ! mv "$tmp" "$file"; then
      rm -f "$tmp"
      rmdir "$lock" 2>/dev/null || true
      GATE_FLOOR_ERROR="could not atomically raise floor file: $file"
      echo "INVALID FLOOR FILE: $GATE_FLOOR_ERROR - refusing to grade" >&2
      return 2
    fi
    echo "coverage ratchet raised: $name $floor -> $count"
    if ! rmdir "$lock" 2>/dev/null; then
      GATE_FLOOR_ERROR="could not release coverage floor lock: $lock"
      echo "INVALID FLOOR LOCK: $GATE_FLOOR_ERROR" >&2
      return 2
    fi
  fi
  return 0
}
