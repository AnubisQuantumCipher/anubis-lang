#!/usr/bin/env bash
# Completion Blueprint Phase 8, Slice 1 — production-linked SecurityLabel correspondence gate.
#
# Two independent observers, one comparison:
#
#   * Rust side. `cargo test` runs the compiler crate's integration test
#     `security_label_correspondence_observer::public_observer_emit_to_env_path`, which
#     calls `anubis_compiler::observe_security_label_correspondence` — the actual production
#     `SecurityLabel::{from_legacy_taint, from_legacy_secret, join, declassified_by,
#     to_legacy_taint, to_legacy_secret}` methods, no shadow reimplementation — over the
#     declared finite abstraction, and writes the canonical TSV corpus to a private path
#     given via `ANUBIS_SECURITY_LABEL_OBSERVATIONS_OUT`.
#
#   * Lean side. `lake exe security_label_observer` in `formal/` runs
#     `Anubis.SecurityLabelObserver.main`, which formats
#     `Anubis.SecurityLabel.observationRows` — the same corpus whose length and no-duplicates
#     are locked by `observationRows_length` and `observationRows_nodup` in
#     `formal/Anubis/SecurityLabel.lean` — over the mechanized model whose theorems are
#     built by `lake build`.
#
# The gate then:
#   1. Refuses schema failure (wrong row count, malformed row, unexpected op, missing op).
#   2. Refuses key duplication or key omission on either side.
#   3. Byte-compares the two files with `cmp` (bit-identical stream).
#   4. Emits exactly one seal-scoreable terminal verdict:
#        SECURITY_LABEL_CORRESPONDENCE_GATE: PASS (...)
#        SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (...)
#
# The gate's `--self-test` runs deterministic negative controls that mutate TEMPORARY COPIES
# of the observed output and re-invoke the comparator; each mutation MUST make the comparator
# FAIL, proving the gate is sensitive to the exact defect classes the mission requires.
# The tracked source tree is never mutated by the self-test.
#
# Usage:
#   bash scripts/run_security_label_correspondence_gate.sh [--out DIR] [--rust-only] [--lean-only]
#   bash scripts/run_security_label_correspondence_gate.sh --self-test
#
# Private outputs. Never writes to a shared repository path unless `--out` is given.
# Honours the mission's private cargo target directory to avoid racing the shared build.

set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

DECLARED_ROW_COUNT=83
declare -a DECLARED_OPS=(
  "from_legacy_taint:4"
  "from_legacy_secret:2"
  "join:49"
  "declassified_by:14"
  "to_legacy_taint:7"
  "to_legacy_secret:7"
)
LEAN_TOOLCHAIN_HOME="${LEAN_TOOLCHAIN_HOME:-$HOME/.elan/bin}"

usage() {
  sed -n '2,32p' "$0"
}

OUT=""
RUN_MODE="normal"      # normal | self-test | rust-only | lean-only
while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      if [[ $# -lt 2 ]]; then
        echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (missing value for --out)" >&2
        exit 2
      fi
      OUT="$2"; shift 2 ;;
    --self-test) RUN_MODE="self-test"; shift ;;
    --rust-only) RUN_MODE="rust-only"; shift ;;
    --lean-only) RUN_MODE="lean-only"; shift ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (unknown flag: $1)" >&2
      exit 2 ;;
  esac
done

if [[ -z "$OUT" ]]; then
  STAMP="$(date +%Y%m%dT%H%M%S)_$$"
  OUT="$ROOT/out/security_label_correspondence/${STAMP}"
fi
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi
mkdir -p "$OUT" || { echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (cannot create --out $OUT)"; exit 2; }

RUST_OBS="$OUT/rust_observations.tsv"
LEAN_OBS="$OUT/lean_observations.tsv"
DIFF_LOG="$OUT/diff.log"
INSTRUMENT="$OUT/instrument.txt"

# ── Instrument identity ─────────────────────────────────────────────────────
if command -v shasum >/dev/null 2>&1; then
  SHASUM_CMD="shasum -a 256"
elif command -v sha256sum >/dev/null 2>&1; then
  SHASUM_CMD="sha256sum"
else
  echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (need shasum or sha256sum)"; exit 2
fi

sha256() { $SHASUM_CMD "$1" | awk '{print $1}'; }

{
  echo "security_label_correspondence_instrument_v1"
  echo "out=$OUT"
  echo "root=$ROOT"
  echo "host=$(uname -s) $(uname -m)"
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "declared_row_count=$DECLARED_ROW_COUNT"
} | tee "$INSTRUMENT" >/dev/null

# ── Observer helpers ────────────────────────────────────────────────────────

# rust_emit <output_path>
# Runs the compiler crate's integration test in --nocapture mode with a private
# CARGO_TARGET_DIR so the shared release target is never disturbed. Writes the
# canonical TSV corpus to $1 (via the ANUBIS_SECURITY_LABEL_OBSERVATIONS_OUT env
# variable that the integration test honours). Prints nothing on success; on
# failure it leaves the cargo log at $OUT/rust_test.log for the operator.
rust_emit() {
  local target="$1"
  local target_dir="${ANUBIS_PHASE8_TARGET_DIR:-/tmp/anubis-phase8-observer-target-$$}"
  mkdir -p "$target_dir"
  local log="$OUT/rust_test.log"
  ANUBIS_SECURITY_LABEL_OBSERVATIONS_OUT="$target" \
  CARGO_TARGET_DIR="$target_dir" \
    cargo test -p anubis-compiler \
      --test security_label_correspondence_observer \
      -- --nocapture --test-threads=1 \
      >"$log" 2>&1
  local rc=$?
  if [[ $rc -ne 0 ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (rust observer test failed; see $log)"
    tail -30 "$log" >&2 || true
    exit 1
  fi
  if [[ ! -s "$target" ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (rust observer produced no output at $target)"
    exit 1
  fi
}

# lean_emit <output_path>
# Runs the Lean 4 executable target `security_label_observer` in formal/. The
# module builds `Anubis.SecurityLabel.observationRows` under the pinned toolchain
# from formal/lean-toolchain (v4.32.0), so a stale build cannot silently ride.
lean_emit() {
  local target="$1"
  local log="$OUT/lean_build.log"
  # Prefer elan's on-path resolution; fall back to the standard elan bin dir.
  local lake_bin=""
  if command -v lake >/dev/null 2>&1; then
    lake_bin="lake"
  elif [[ -x "$LEAN_TOOLCHAIN_HOME/lake" ]]; then
    lake_bin="$LEAN_TOOLCHAIN_HOME/lake"
  else
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (lake not found; install elan or set LEAN_TOOLCHAIN_HOME)"
    exit 2
  fi
  local orig_path="$PATH"
  export PATH="$LEAN_TOOLCHAIN_HOME:$PATH"
  (
    cd "$ROOT/formal" && "$lake_bin" exe security_label_observer "$target"
  ) >"$log" 2>&1
  local rc=$?
  export PATH="$orig_path"
  if [[ $rc -ne 0 ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (lean observer failed; see $log)"
    tail -30 "$log" >&2 || true
    exit 1
  fi
  if [[ ! -s "$target" ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (lean observer produced no output at $target)"
    exit 1
  fi
}

# ── Schema validation (applies to both files independently) ────────────────

# validate_stream <path>
# Prints the observed row count for use in the terminal verdict on PASS.
# Exits nonzero (via caller trap) on any schema violation.
validate_stream() {
  local file="$1"
  local label="$2"
  local rows
  rows="$(wc -l <"$file" | tr -d ' ')"
  if [[ "$rows" != "$DECLARED_ROW_COUNT" ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL ($label row count $rows != declared $DECLARED_ROW_COUNT; file=$file)"
    exit 1
  fi
  # Per-op counts must match the declared abstraction exactly.
  local op count observed
  for spec in "${DECLARED_OPS[@]}"; do
    op="${spec%%:*}"
    count="${spec##*:}"
    observed="$(grep -cE "^${op}"$'\t' "$file" || true)"
    if [[ "$observed" != "$count" ]]; then
      echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL ($label op '$op' row count $observed != declared $count; file=$file)"
      exit 1
    fi
  done
  # Every row must be four tab-separated fields, no blanks. `awk` writes to /dev/null
  # so failures surface only on FS_NF != 4 for at least one row.
  local malformed
  malformed="$(awk -F'\t' 'NF != 4 { print NR; exit }' "$file")"
  if [[ -n "$malformed" ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL ($label row $malformed has NF != 4; file=$file)"
    exit 1
  fi
  # Keys (op|arg1|arg2) must be unique.
  local dup
  dup="$(awk -F'\t' '{print $1 "|" $2 "|" $3}' "$file" | sort | uniq -d | head -1)"
  if [[ -n "$dup" ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL ($label duplicate key `$dup`; file=$file)"
    exit 1
  fi
  # Every op is in the declared abstraction; no stray ops.
  local unexpected
  unexpected="$(awk -F'\t' '{print $1}' "$file" | sort -u \
    | comm -23 - <(printf '%s\n' "${DECLARED_OPS[@]}" | awk -F: '{print $1}' | sort -u))"
  if [[ -n "$unexpected" ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL ($label unexpected op(s): $(echo $unexpected); file=$file)"
    exit 1
  fi
}

# compare_streams <rust_path> <lean_path>
# Byte-compares the two observations; writes a human-readable diff to $DIFF_LOG on failure.
compare_streams() {
  if cmp -s "$1" "$2"; then
    return 0
  fi
  diff -u "$1" "$2" >"$DIFF_LOG" 2>&1 || true
  echo "SECURITY_LABEL_CORRESPONDENCE_GATE: FAIL (Rust and Lean streams differ; diff at $DIFF_LOG)"
  echo "--- top of diff ---" >&2
  head -30 "$DIFF_LOG" >&2 || true
  exit 1
}

# ── Self-test / negative controls ───────────────────────────────────────────
# Every mutation happens on a temporary copy of the CURRENT observed output.
# No tracked file is ever modified. Every mutation MUST make either the schema
# validator or `cmp` FAIL — if any mutation passes silently, the gate is
# insensitive to the exact defect class and the self-test refuses the seal.

if [[ "$RUN_MODE" == "self-test" ]]; then
  {
    echo "== SECURITY_LABEL_CORRESPONDENCE self-test =="
    date -u +%Y-%m-%dT%H:%M:%SZ
    echo "out=$OUT"
  } | tee "$OUT/selftest.log"

  ST_ROOT="$OUT/selftest"
  mkdir -p "$ST_ROOT"

  # Materialise Rust + Lean observations once; every mutation runs on a copy.
  rust_emit "$ST_ROOT/rust_observations.tsv"
  lean_emit "$ST_ROOT/lean_observations.tsv"

  # Sanity: unmutated pair must PASS validation and cmp — otherwise every mutation
  # test below is vacuous.
  validate_stream "$ST_ROOT/rust_observations.tsv" "self-test.rust.baseline"
  validate_stream "$ST_ROOT/lean_observations.tsv" "self-test.lean.baseline"
  if ! cmp -s "$ST_ROOT/rust_observations.tsv" "$ST_ROOT/lean_observations.tsv"; then
    echo "SECURITY_LABEL_CORRESPONDENCE_SELFTEST: FAIL (baseline pair diverges; cannot exercise mutations)"
    exit 1
  fi
  echo "self-test baseline PASS ($(sha256 "$ST_ROOT/rust_observations.tsv"))" | tee -a "$OUT/selftest.log"

  # expect_fail <name> <description> <mutation_cmd...>
  # Runs the mutation, then applies validate_stream + compare_streams to the
  # mutated pair, redirecting all gate output to a log file. It must exit
  # NONZERO (mutation detected). Each mutation runs in a subshell so `exit 1`
  # inside compare_streams doesn't bubble out of the self-test.
  self_pass=0
  self_fail=0

  expect_fail() {
    local name="$1"; shift
    local desc="$1"; shift
    local rust_file="$ST_ROOT/mut_${name}_rust.tsv"
    local lean_file="$ST_ROOT/mut_${name}_lean.tsv"
    cp "$ST_ROOT/rust_observations.tsv" "$rust_file"
    cp "$ST_ROOT/lean_observations.tsv" "$lean_file"
    # Mutation callback: receives the two filenames as $1, $2 in a subshell.
    local mut_log="$ST_ROOT/mut_${name}.mutation.log"
    ( "$@" "$rust_file" "$lean_file" ) >"$mut_log" 2>&1
    local mrc=$?
    if [[ $mrc -ne 0 ]]; then
      echo "self-test '$name' mutation itself failed rc=$mrc: $desc" | tee -a "$OUT/selftest.log"
      self_fail=$((self_fail+1))
      return
    fi
    local out_log="$ST_ROOT/mut_${name}.gate.log"
    ( set +e
      validate_stream "$rust_file" "self-test.rust.$name" 2>&1
      _v_rc=$?
      if [[ $_v_rc -ne 0 ]]; then exit $_v_rc; fi
      validate_stream "$lean_file" "self-test.lean.$name" 2>&1
      _v_rc=$?
      if [[ $_v_rc -ne 0 ]]; then exit $_v_rc; fi
      compare_streams "$rust_file" "$lean_file" 2>&1
    ) >"$out_log" 2>&1
    local grc=$?
    if [[ $grc -ne 0 ]]; then
      echo "self-test PASS  [$name]  $desc  (gate rejected; rc=$grc)" | tee -a "$OUT/selftest.log"
      self_pass=$((self_pass+1))
    else
      echo "self-test FAIL  [$name]  $desc  (gate silently accepted a mutation)" | tee -a "$OUT/selftest.log"
      cat "$out_log" >&2 || true
      self_fail=$((self_fail+1))
    fi
  }

  # ── Mutation defs ───────────────────────────────────────────────────────
  # Each callback edits its own copy of the Rust and/or Lean file in place.

  # C1: alter one Rust output row → cmp fails.
  mut_rust_row_altered() {
    local rust="$1"; local lean="$2"
    # Change the load-bearing "join left-bias" row's OUTPUT column on the Rust
    # side only. Both sides currently emit Labeled(some:s1); flip Rust to s2.
    perl -0777 -i -pe 's{^join\tLabeled\(some:s1\)\tLabeled\(some:s2\)\tLabeled\(some:s1\)$}{join\tLabeled(some:s1)\tLabeled(some:s2)\tLabeled(some:s2)}m' "$rust"
  }
  expect_fail rust_row_altered \
    "altering ONE Rust output row → cmp rejects (Rust-side lying about join first-wins bias)" \
    mut_rust_row_altered

  # C2: alter one Lean output row → cmp fails.
  mut_lean_row_altered() {
    local rust="$1"; local lean="$2"
    perl -0777 -i -pe 's{^to_legacy_taint\tUnknown\(some:r1\)\t-\tLegacy\(tainted=true,source=some:unknown-label\)$}{to_legacy_taint\tUnknown(some:r1)\t-\tLegacy(tainted=false,source=none)}m' "$lean"
  }
  expect_fail lean_row_altered \
    "altering ONE Lean output row (Unknown→Clean adapter) → cmp rejects (Lean-side breaking fail-closed contract)" \
    mut_lean_row_altered

  # C3: delete one row on the Rust side → schema validator rejects wrong count.
  mut_rust_row_deleted() {
    local rust="$1"; local lean="$2"
    # Delete the first `to_legacy_secret` row.
    perl -i -ne 'print unless (!$done and /^to_legacy_secret\t/ and ($done=1))' "$rust"
  }
  expect_fail rust_row_deleted \
    "deleting ONE Rust row → schema validator rejects wrong row count (silent corpus shrinkage)" \
    mut_rust_row_deleted

  # C4: duplicate one row on the Lean side → schema validator rejects duplicate key.
  mut_lean_duplicate_row() {
    local rust="$1"; local lean="$2"
    # Duplicate the load-bearing declassify Unknown row.
    local row='declassified_by	Unknown(some:r1)	true	Unknown(some:r1)'
    grep -q "^${row}$" "$lean" || { echo "self-test fixture drift: expected Lean row missing"; return 1; }
    printf '%s\n' "$row" >>"$lean"
  }
  expect_fail lean_duplicate_row \
    "duplicating ONE Lean row → schema validator rejects duplicate (op,arg1,arg2) key" \
    mut_lean_duplicate_row

  # C5: omit an entire input class on the Rust side by removing every `join`
  # row with a specific left operand → schema validator rejects wrong join
  # row count.
  mut_rust_omit_input_class() {
    local rust="$1"; local lean="$2"
    perl -i -ne 'print unless /^join\tUnknown\(none\)\t/' "$rust"
  }
  expect_fail rust_omit_input_class \
    "removing every join row with a specific input class (Unknown(none) as left) → schema validator rejects op count" \
    mut_rust_omit_input_class

  # C6: replace Unknown→something-clean in the confidentiality adapter on Rust
  # side. Reproduces the exact defect class the mission calls out ("Unknown
  # must not become secret=false") on the wire.
  mut_rust_unknown_to_clean_secret() {
    local rust="$1"; local lean="$2"
    perl -0777 -i -pe 's{^to_legacy_secret\tUnknown\(some:r1\)\t-\ttrue$}{to_legacy_secret\tUnknown(some:r1)\t-\tfalse}m' "$rust"
  }
  expect_fail rust_unknown_to_clean_secret \
    "changing to_legacy_secret(Unknown) from true to false on Rust side → cmp rejects (violates fail-closed adapter)" \
    mut_rust_unknown_to_clean_secret

  # C7: overclaim full commutativity by rewriting the left-bias witness on the
  # Lean side to match a hypothetical FULLY commutative implementation. This
  # exercises the mission's guardrail: full-record commutativity is FALSE
  # (`join_full_not_commutative`), and any observer that claims otherwise is
  # a fabricated claim the gate must reject.
  mut_lean_fake_full_commutativity() {
    local rust="$1"; local lean="$2"
    perl -0777 -i -pe 's{^join\tLabeled\(some:s1\)\tLabeled\(some:s2\)\tLabeled\(some:s1\)$}{join\tLabeled(some:s1)\tLabeled(some:s2)\tLabeled(some:s2)}m' "$lean"
  }
  expect_fail lean_fake_full_commutativity \
    "Lean falsely claims full-commutative join on the (s1,s2) witness → cmp rejects (guardrail against overclaim)" \
    mut_lean_fake_full_commutativity

  # C8: introduce a malformed (three-field) row on the Lean side → schema
  # validator rejects NF != 4.
  mut_lean_malformed_row() {
    local rust="$1"; local lean="$2"
    perl -i -pe 's/^(to_legacy_secret\tClean\t-)\tfalse$/$1/m' "$lean"
  }
  expect_fail lean_malformed_row \
    "dropping the OUTPUT column from a Lean row → schema validator rejects NF != 4" \
    mut_lean_malformed_row

  # C9: replace one op prefix with an unknown op → schema validator rejects
  # unexpected op AND row-count mismatch on the correct op.
  mut_rust_unknown_op() {
    local rust="$1"; local lean="$2"
    perl -0777 -i -pe 's{^to_legacy_secret\tClean\t-\tfalse$}{to_legacy_secret_wrong\tClean\t-\tfalse}m' "$rust"
  }
  expect_fail rust_unknown_op \
    "renaming one row's op to an undeclared name → schema validator rejects unexpected op" \
    mut_rust_unknown_op

  echo "self-test summary: pass=$self_pass fail=$self_fail" | tee -a "$OUT/selftest.log"
  if [[ "$self_fail" -eq 0 && "$self_pass" -gt 0 ]]; then
    echo "SECURITY_LABEL_CORRESPONDENCE_SELFTEST: PASS (pass=$self_pass fail=0)"
    exit 0
  fi
  echo "SECURITY_LABEL_CORRESPONDENCE_SELFTEST: FAIL (pass=$self_pass fail=$self_fail)"
  exit 1
fi

# ── Normal mode: emit, validate, compare, verdict ──────────────────────────
if [[ "$RUN_MODE" != "lean-only" ]]; then
  rust_emit "$RUST_OBS"
  validate_stream "$RUST_OBS" "rust"
fi
if [[ "$RUN_MODE" != "rust-only" ]]; then
  lean_emit "$LEAN_OBS"
  validate_stream "$LEAN_OBS" "lean"
fi

if [[ "$RUN_MODE" == "rust-only" ]]; then
  echo "SECURITY_LABEL_CORRESPONDENCE_GATE: PASS (rust-only; rows=$DECLARED_ROW_COUNT sha256=$(sha256 "$RUST_OBS"))"
  exit 0
fi
if [[ "$RUN_MODE" == "lean-only" ]]; then
  echo "SECURITY_LABEL_CORRESPONDENCE_GATE: PASS (lean-only; rows=$DECLARED_ROW_COUNT sha256=$(sha256 "$LEAN_OBS"))"
  exit 0
fi

compare_streams "$RUST_OBS" "$LEAN_OBS"

RUST_SHA="$(sha256 "$RUST_OBS")"
LEAN_SHA="$(sha256 "$LEAN_OBS")"
{
  echo "rust_observations_sha256=$RUST_SHA"
  echo "lean_observations_sha256=$LEAN_SHA"
} | tee -a "$INSTRUMENT" >/dev/null

echo "SECURITY_LABEL_CORRESPONDENCE_GATE: PASS (rows=$DECLARED_ROW_COUNT rust_sha=$RUST_SHA lean_sha=$LEAN_SHA out=$OUT)"
exit 0
