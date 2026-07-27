#!/usr/bin/env bash
# Shared fail-closed primitives for fixture gates. Callers retain their own
# domain-specific execution and reporting; these functions only enforce the
# invariants that must not vary between harnesses.

# parse_expectation FILE BASENAME [NAME_POLICY]
# Result variables: GATE_EXPECT, GATE_MALFORMED.
# NAME_POLICY "accept_reject" binds *_accepts/*_rejects to their headers.
parse_expectation() {
  local file="$1" base="$2" name_policy="${3:-none}"
  GATE_EXPECT="$(grep -oE 'EXPECT: (PASS|FAIL)' "$file" 2>/dev/null \
    | head -1 | awk '{print $2}' || true)"
  GATE_MALFORMED=""

  if [[ -z "$GATE_EXPECT" ]]; then
    GATE_MALFORMED="missing EXPECT: header"
    return 1
  fi

  if [[ "$name_policy" == "accept_reject" ]]; then
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

  [[ -z "$GATE_MALFORMED" ]]
}

# score_fixture EXPECTED ACTUAL
# Result variable: GATE_FIXTURE_STATUS.
score_fixture() {
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
  local count="$1" description="$2"
  if [[ ! "$count" =~ ^[0-9]+$ || "$count" -eq 0 ]]; then
    echo "EMPTY CORPUS: no fixtures matched $description - refusing to report PASS" >&2
    return 1
  fi
  return 0
}

# finalize TOTAL PASSED FAILED [INCOMPLETE]
# Result variable: GATE_FINAL_STATUS (PASS, FAIL, or INCOMPLETE).
finalize() {
  local total="$1" passed="$2" failed="$3" incomplete="${4:-0}"
  GATE_FINAL_STATUS="FAIL"
  require_nonempty_corpus "$total" "the configured corpus" || return 1
  if [[ "$failed" -gt 0 ]]; then
    return 1
  fi
  if [[ "$incomplete" -gt 0 || $((passed + failed + incomplete)) -ne "$total" ]]; then
    GATE_FINAL_STATUS="INCOMPLETE"
    return 2
  fi
  GATE_FINAL_STATUS="PASS"
  return 0
}

