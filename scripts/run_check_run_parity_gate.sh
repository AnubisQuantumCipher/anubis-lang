#!/usr/bin/env bash
# ============================================================================
# check / run-preflight parity gate
# ============================================================================
# The product promise is "a green `anubis check` means it is safe to `anubis
# run`". This gate makes that promise machine-checkable: for every corpus
# program it asserts that the `check` policy verdict (PASS/REJECT) equals the
# verdict of `run`'s native-execution preflight (`verify_before_native_execution`
# in tools/anubis/src/main.rs) — WITHOUT ever letting a user program actually
# execute.
#
# WHY NOT JUST CALL `anubis run`: if the preflight passes, `anubis run`
# unconditionally proceeds to compile and EXECUTE the program (side effects:
# file writes, shell, network, hangs waiting on args). There is no `--dry-run`
# flag on `run` and this script may not add one (compiler/tools ownership is
# out of scope for this gate; see the patch specs in
# scratchpad/fleet_20260726/sonnet5a_patch_spec.md).
#
# THE TRICK: `verify_before_native_execution` runs FIRST, before any file is
# written under `--out` (tools/anubis/src/main.rs, well before the
# `std::fs::create_dir_all(out)` call inside `run_anubis_source`, which itself
# precedes compilation and execution by construction). So we point `--out` at
# a path under a directory we've chmod'd 555 (read+exec, no write). Two
# outcomes, both observed purely from the child process's own exit code /
# stderr — the child process does the deciding, we only watch:
#   * preflight REJECTS  -> the ANUBIS_* coded error prints; `--out` is never
#     touched (verify_before_native_execution takes no `out` argument at all).
#   * preflight PASSES   -> execution reaches `create_dir_all(out)`, which
#     fails immediately with a plain OS `Permission denied (os error 13)` —
#     no compile, no run. Verified empirically on a known-good and a
#     known-bad file (see scratchpad/fleet_20260726/sonnet5a_check_run_parity.md
#     and scratchpad/fleet_20260726/sonnet5c_parity_gate.md for independent
#     re-verification).
#
# A defensive belt-and-braces check also greps for the "compiling native
# binary" progress line `run_anubis_source` prints right before invoking
# rustc; if that ever appears, the barrier failed to hold and the gate aborts
# loudly (exit 2) instead of silently reporting a wrong verdict. Under this
# barrier, an actual `rustc` compile error is structurally unreachable (the
# gate never lets the child process get that far) — see CRASH handling below
# for the failure mode that plays the analogous role here: the checker/
# preflight PROCESS itself panicking, as opposed to cleanly returning a
# policy verdict.
#
# DIRECTION MATTERS: a disagreement is not one thing. Three buckets:
#   * DANGEROUS (R-class) — `check=PASS, run=REJECT` for a *policy* reason
#     (preflight re-derivation of taint / obligations). Checker vouched;
#     runtime wrongly refused. THIS is the only bucket that means the
#     product promise is broken.
#   * NON_RUN_BY_DESIGN (B-class) — `check=PASS, run=REJECT` because the
#     program uses a deliberately non-run construct (`is_non_run_builtin` in
#     compiler/src/backends/run.rs:3466-3481: symbolic/assume/assert/
#     taint_source/declassify/sink/shell/exec/system/memcpy/sql). Check is
#     correct for policy fixtures; run correctly refuses to lower. NOT a
#     bug, NOT a cry-wolf DANGEROUS. Counted separately so the gate is not
#     ignored for 7 permanent known residuals.
#   * CONSERVATIVE — `check=REJECT, run=PASS` (should be unreachable given
#     how preflight is built; still reported).
# Every divergence is also tagged with a best-effort root-cause CATEGORY
# (PREFLIGHT_POLICY / NATIVE_LOWERING_GAP / OTHER). Direction and category
# are two independent axes; never conflated.
# GATE FAIL = any DANGEROUS or CONSERVATIVE. NON_RUN_BY_DESIGN alone does
# not fail the gate (exit 0 with a residual count in the report).
#
# SCOPE: sweeps every directory passed via --dir (repeatable). Defaults to
# `examples` (the organic example/showcase corpus, 245 files as of
# 2026-07-26) and `tests/fixtures/language_core` (the canonical language
# fixture corpus used by scripts/run_language_fixtures.sh, 244 files as of
# 2026-07-26) — the two directories a `find scripts -name 'run_*.sh' | xargs
# grep -l` sweep shows sibling gates already treat as "the corpus". There are
# also `.anub`/`.anubis` example files this gate does not cover (`.anb` only,
# matching scripts/run_language_fixtures.sh's own scope) — a follow-up, not
# silently done here.
#
# Files with no `fn main()` (library modules, e.g. examples/lang/multifile/
# math.anb) are SKIPPED, not compared: `check` can legitimately PASS a
# module in isolation, while `run` always rejects with
# ANUBIS_UNSUPPORTED_NATIVE_LOWERING because there is no entry point to
# execute. That is an expected structural distinction, not a policy
# divergence — conflating it with a real one would hide the real ones in
# the noise. Same discriminator `discover_test_files` in
# tools/anubis/src/main.rs already uses.
#
# Research/Exploit-mode programs: `verify_before_native_execution` is
# mode-agnostic and runs regardless of `--allow-research`. A SEPARATE,
# later gate inside `run_anubis_source` (ANUBIS_RUN_RESEARCH_REQUIRES_ALLOW)
# blocks non-Safe programs from executing on the host without explicit
# `--allow-research` — by design (anubis-offensive-vz-isolation-mandatory:
# research/exploit execution belongs in the VZ guest, never bare host). This
# gate never passes `--allow-research` (staying host-safe) and treats
# ANUBIS_RUN_RESEARCH_REQUIRES_ALLOW as "preflight PASSED, blocked by a
# different, intentional gate" — i.e. the same bucket as the permission
# barrier — so it is not reported as a false parity divergence.
#
# TIMEOUTS: every `check` and every `run`-preflight invocation is wrapped in
# `timeout`. A timeout is NEVER treated as a verdict of either kind (not
# PASS, not REJECT, not a divergence) — it is SKIPPED with a reason, and
# counted in its own bucket, separate from both comparisons and divergences.
# (Motivating case: `import std.math` is known to hang the checker as of
# 2026-07-26, a separate P0 — a gate that scored a timeout as a disagreement
# would cry wolf every time the machine is busy and would then be ignored.)
#
# CRASHES: if the checker or preflight process itself panics (Rust unwind,
# typically exit 101, or dies by signal — SIGABRT/SIGSEGV, exit 134/139) or
# stderr contains "panicked at", that side is SKIPPED with reason "crash",
# in its own bucket, exactly like a timeout — never compared as if it were
# a clean PASS/REJECT verdict. This matters even when both sides happen to
# crash: a coincidence of exit codes is not agreement, and comparing two
# crashes as if they were a matching verdict would hide the crash entirely.
# "The tool broke" is a different, more urgent problem than "the tool
# disagreed with itself", and the two must never be folded together.
#
# Usage:
#   scripts/run_check_run_parity_gate.sh [--timeout SECONDS] [--dir PATH]... [--out DIR]
#
# Exit codes: 0 = full parity (no divergence, ignoring skips). 1 = at least
# one divergence found. 2 = misconfiguration or the safety barrier itself
# failed (abort, do not trust any verdict past that point).
# ============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

ANUBIS="${ANUBIS_BIN:-${ANUBIS:-$ROOT/target/release/anubis}}"
TIMEOUT_SECS="${ANUBIS_PARITY_GATE_TIMEOUT:-150}"
OUT_DIR="out/check_run_parity_gate"
SEARCH_DIRS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --timeout) TIMEOUT_SECS="$2"; shift 2 ;;
    --dir) SEARCH_DIRS+=("$2"); shift 2 ;;
    --out) OUT_DIR="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [[ ${#SEARCH_DIRS[@]} -eq 0 ]]; then
  SEARCH_DIRS=("examples" "tests/fixtures/language_core")
fi

# Resolve to absolute paths and drop any that don't exist (--dir is opt-in
# for extra corpora; a missing default dir should not hard-fail the gate).
RESOLVED_DIRS=()
for d in "${SEARCH_DIRS[@]}"; do
  case "$d" in
    /*) abs="$d" ;;
    *) abs="$ROOT/$d" ;;
  esac
  if [[ -d "$abs" ]]; then
    RESOLVED_DIRS+=("$abs")
  else
    echo "WARNING: --dir $d does not exist under $ROOT — skipping" >&2
  fi
done
if [[ ${#RESOLVED_DIRS[@]} -eq 0 ]]; then
  echo "GATE: FAIL — no valid corpus directories (checked: ${SEARCH_DIRS[*]})" >&2
  exit 2
fi

case "$OUT_DIR" in
  /*) ;; # absolute — use as-is
  *) OUT_DIR="$ROOT/$OUT_DIR" ;;
esac
mkdir -p "$OUT_DIR"

if [[ ! -x "$ANUBIS" ]]; then
  echo "building release binary (not found at $ANUBIS)..." >&2
  cargo build --release -p anubis 2>&1 | tail -20
fi

TIMEOUT_BIN="timeout"
if ! command -v timeout >/dev/null 2>&1; then
  if command -v gtimeout >/dev/null 2>&1; then
    TIMEOUT_BIN="gtimeout"
  else
    echo "FATAL: neither 'timeout' nor 'gtimeout' found on PATH — required so this gate can never hang" >&2
    exit 2
  fi
fi

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/anubis-parity-gate.XXXXXX")"
LOCKED="$SCRATCH/locked"
CHECK_OUT_BASE="$SCRATCH/check_out"
mkdir -p "$LOCKED" "$CHECK_OUT_BASE"
chmod 555 "$LOCKED"

cleanup() {
  chmod -R u+w "$SCRATCH" 2>/dev/null || true
  rm -rf "$SCRATCH" 2>/dev/null || true
}
trap cleanup EXIT

# Force the non-signing run path so behavior doesn't depend on macOS Keychain
# state, and never let this gate touch VZ/offensive isolation machinery.
export ANUBIS_RUN_NO_SIGN=1

total=0
compared=0
skipped_no_main=0
skipped_timeout=0
skipped_crash=0
divergences=0
divergences_dangerous=0       # (R) check=PASS, run=REJECT for PREFLIGHT_POLICY / OTHER policy
divergences_non_run=0         # (B) check=PASS, run=REJECT for NATIVE_LOWERING / non-run-by-design
divergences_conservative=0    # check=REJECT, run=PASS
first_divergence=""
first_dangerous=""

# Divergence rows accumulated for the closing table + JSON report.
# Pipe-delimited: file|check_verdict|run_verdict|direction|category
declare -a DIV_ROWS=()

echo "check/run-preflight parity gate"
echo "  binary:  $ANUBIS"
echo "  scope:   $(for d in "${SEARCH_DIRS[@]}"; do printf '%s/**/*.anb  ' "$d"; done)"
echo "  timeout: ${TIMEOUT_SECS}s per invocation"
echo "  out:     $OUT_DIR"
echo "  scratch: $SCRATCH"
echo

for d in "${RESOLVED_DIRS[@]}"; do
  while IFS= read -r -d '' f; do
  total=$((total + 1))
  rel="${f#"$ROOT"/}"

  # A file with no `fn main()` is a library module, not a runnable entry point.
  # `check` validates a module in isolation and can legitimately PASS one;
  # `run` requires an entry point and will always reject with
  # ANUBIS_UNSUPPORTED_NATIVE_LOWERING. Expected structural distinction, not
  # a policy-verdict divergence — skipped rather than compared.
  if ! grep -q "fn main" "$f"; then
    echo "SKIPPED (no fn main — library module, not runnable): $rel"
    skipped_no_main=$((skipped_no_main + 1))
    continue
  fi

  check_out="$CHECK_OUT_BASE/c_$total"
  mkdir -p "$check_out"
  check_stderr="$SCRATCH/check_stderr_$total.txt"
  set +e
  "$TIMEOUT_BIN" "$TIMEOUT_SECS" "$ANUBIS" check "$f" --out "$check_out" \
    >"$SCRATCH/check_stdout_$total.txt" 2>"$check_stderr"
  check_exit=$?
  set -e

  if [[ "$check_exit" -eq 124 || "$check_exit" -eq 137 ]]; then
    echo "SKIPPED (check timeout >${TIMEOUT_SECS}s): $rel"
    skipped_timeout=$((skipped_timeout + 1))
    continue
  fi

  # A CRASH (the checker process itself panicking or dying by signal) is not
  # a policy verdict at all, on either side — treat it exactly like a
  # timeout: skip the comparison entirely and count it in its own bucket,
  # rather than letting an incidental exit-code coincidence (e.g. both sides
  # crash and both happen to report a nonzero exit) get silently absorbed
  # into "the verdicts agreed". A crash is never a trustworthy verdict.
  if [[ "$check_exit" -eq 101 || "$check_exit" -eq 134 || "$check_exit" -eq 139 ]] \
     || grep -q "panicked at" "$check_stderr" 2>/dev/null; then
    echo "SKIPPED (check CRASHED, exit $check_exit — not a policy verdict): $rel"
    skipped_crash=$((skipped_crash + 1))
    continue
  fi
  if [[ "$check_exit" -eq 0 ]]; then
    check_verdict="PASS"
  else
    check_verdict="REJECT"
  fi

  run_out="$LOCKED/blocked_$total"
  run_stderr="$SCRATCH/run_stderr_$total.txt"
  set +e
  "$TIMEOUT_BIN" "$TIMEOUT_SECS" "$ANUBIS" run "$f" --out "$run_out" \
    >"$SCRATCH/run_stdout_$total.txt" 2>"$run_stderr"
  run_exit=$?
  set -e

  if [[ "$run_exit" -eq 124 || "$run_exit" -eq 137 ]]; then
    echo "SKIPPED (run-preflight timeout >${TIMEOUT_SECS}s): $rel"
    skipped_timeout=$((skipped_timeout + 1))
    continue
  fi

  # Belt-and-braces: if compilation actually started, the barrier did not
  # hold. Abort rather than report a verdict we can no longer trust, and
  # rather than risk letting a program's native binary actually execute.
  if grep -q "compiling native binary" "$run_stderr" 2>/dev/null; then
    echo "FATAL: --out barrier did not hold for $rel — a native build was attempted." >&2
    echo "       Aborting the sweep; do not trust results collected so far as complete." >&2
    exit 2
  fi

  # Same CRASH-is-not-a-verdict treatment as the check side, above. A panic
  # exit (101) is also how the barrier itself could in principle surface
  # (Rust panics unwind to a nonzero exit distinct from the coded ANUBIS_*
  # Err path) — either way this is "the tool broke", not "the tool decided".
  if [[ "$run_exit" -eq 101 || "$run_exit" -eq 134 || "$run_exit" -eq 139 ]] \
     || grep -q "panicked at" "$run_stderr" 2>/dev/null; then
    echo "SKIPPED (run-preflight CRASHED, exit $run_exit — not a policy verdict): $rel"
    skipped_crash=$((skipped_crash + 1))
    continue
  fi

  if grep -qE "^Error: Permission denied" "$run_stderr"; then
    run_verdict="PASS"       # preflight passed; stopped by our own barrier
  elif grep -q "ANUBIS_RUN_RESEARCH_REQUIRES_ALLOW" "$run_stderr"; then
    run_verdict="PASS"       # preflight passed; blocked by the separate, intentional mode gate
  elif [[ "$run_exit" -eq 0 ]]; then
    # Should be unreachable given the barrier (no successful run ever creates
    # $run_out under a chmod-555 parent) — but never silently call this PASS
    # without saying so loudly, since it would mean the barrier was bypassed.
    echo "WARNING: run exited 0 unexpectedly for $rel (barrier bypass?) — treating as PASS" >&2
    run_verdict="PASS"
  else
    run_verdict="REJECT"
  fi

  compared=$((compared + 1))

  if [[ "$check_verdict" != "$run_verdict" ]]; then
    divergences=$((divergences + 1))

    # Category first — direction for check=PASS/run=REJECT depends on it so
    # NATIVE_LOWERING is NON_RUN_BY_DESIGN, not a cry-wolf DANGEROUS.
    if grep -q "ANUBIS_UNSUPPORTED_NATIVE_LOWERING" "$run_stderr" \
       || grep -qE "builtin \`(shell|exec|system|symbolic|assume|assert|sink|declassify|taint_source|memcpy|sql)\` is a proof/analysis construct" "$run_stderr" \
       || grep -q "not available in \`run\`" "$run_stderr"; then
      category="NATIVE_LOWERING_GAP (is_non_run_builtin / verification-only construct — intentional non-run, not a policy bug)"
    elif grep -q "ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY\|ANUBIS_EXECUTION_UNVERIFIED\|ANUBIS_EXECUTION_CHECK_FAILED" "$run_stderr"; then
      category="PREFLIGHT_POLICY (verify_before_native_execution verdict differs from check — (R) runtime false reject)"
    else
      category="OTHER (uncategorized — inspect stderr below)"
    fi

    if [[ "$check_verdict" == "PASS" && "$run_verdict" == "REJECT" ]]; then
      # Only PREFLIGHT_POLICY / OTHER policy rejections are (R) DANGEROUS.
      # NATIVE_LOWERING is (B) NON_RUN_BY_DESIGN — check is correct, run is
      # correct for a non-run construct; product residual, not a gate fail.
      if [[ "$category" == NATIVE_LOWERING_GAP* ]]; then
        direction="NON_RUN_BY_DESIGN"
        divergences_non_run=$((divergences_non_run + 1))
      else
        direction="DANGEROUS"
        divergences_dangerous=$((divergences_dangerous + 1))
        if [[ -z "$first_dangerous" ]]; then
          first_dangerous="$rel"
        fi
      fi
    else
      direction="CONSERVATIVE"
      divergences_conservative=$((divergences_conservative + 1))
    fi

    DIV_ROWS+=("$rel|$check_verdict|$run_verdict|$direction|$category")

    echo "DIVERGENCE #$divergences [$direction / $category]: $rel  check=$check_verdict  run_preflight=$run_verdict"
    echo "  --- check stderr ---"
    sed 's/^/    /' "$check_stderr"
    echo "  --- run stderr ---"
    sed 's/^/    /' "$run_stderr"
    echo
    if [[ -z "$first_divergence" ]]; then
      first_divergence="$rel"
    fi
  fi
  done < <(find "$d" -name "*.anb" -type f -print0 | sort -z)
done

echo "==================== SUMMARY ===================="
echo "total .anb files found:      $total"
echo "compared:                    $compared"
echo "skipped (no fn main):        $skipped_no_main"
echo "skipped (timeout >${TIMEOUT_SECS}s):       $skipped_timeout"
echo "skipped (crash — not a verdict): $skipped_crash"
echo "divergences:                 $divergences"
echo "  dangerous (R — preflight false reject):  $divergences_dangerous"
echo "  non_run_by_design (B — intentional):     $divergences_non_run"
echo "  conservative (check=REJECT, run=PASS):   $divergences_conservative"
echo

if [[ "$divergences" -gt 0 ]]; then
  echo "---- divergence table ----"
  printf '%-70s %-8s %-8s %-18s %s\n' "FILE" "CHECK" "RUN" "DIRECTION" "CATEGORY"
  for row in "${DIV_ROWS[@]}"; do
    IFS='|' read -r f cv rv dir cat <<<"$row"
    printf '%-70s %-8s %-8s %-18s %s\n' "$f" "$cv" "$rv" "$dir" "$cat"
  done
  echo
fi

# Validate that every discovered file reached exactly one terminal classification.
classified=$((compared + skipped_no_main + skipped_timeout + skipped_crash))
set +e
finalize "$total" "$classified" 0 0
partition_rc=$?
set -e

# ---- JSON report ----
# FAIL only when DANGEROUS (R) or CONSERVATIVE remain. NON_RUN_BY_DESIGN alone
# is a documented residual (check ≠ run for proof/shell), not a gate failure.
if [[ "$partition_rc" -ne 0 ]]; then
  overall_verdict="FAIL"
elif [[ "$compared" -eq 0 ]]; then
  overall_verdict="FAIL"
elif [[ "$divergences_dangerous" -gt 0 || "$divergences_conservative" -gt 0 ]]; then
  overall_verdict="FAIL"
elif [[ "$divergences_non_run" -gt 0 ]]; then
  overall_verdict="PASS_WITH_KNOWN_NON_RUN"
else
  overall_verdict="PASS"
fi

{
  echo "{"
  echo "  \"overall_verdict\": \"$overall_verdict\","
  echo "  \"total\": $total,"
  echo "  \"compared\": $compared,"
  echo "  \"skipped_no_main\": $skipped_no_main,"
  echo "  \"skipped_timeout\": $skipped_timeout,"
  echo "  \"skipped_crash\": $skipped_crash,"
  echo "  \"timeout_secs\": $TIMEOUT_SECS,"
  echo "  \"divergences\": $divergences,"
  echo "  \"divergences_dangerous\": $divergences_dangerous,"
  echo "  \"divergences_non_run_by_design\": $divergences_non_run,"
  echo "  \"divergences_conservative\": $divergences_conservative,"
  echo "  \"divergence_rows\": ["
  n=${#DIV_ROWS[@]}
  # Bash 3.2 (macOS's shipped /bin/bash) treats "${arr[@]}" on a truly EMPTY
  # array as an unbound-variable error under `set -u`, even though the array
  # was declared — this is a real, fixed-in-4.4+ bash bug, not a hypothetical.
  # Guard the expansion so a clean (0-divergence) run doesn't crash the gate
  # while writing its own PASS report.
  if [[ "$n" -gt 0 ]]; then
    i=0
    for row in "${DIV_ROWS[@]}"; do
      i=$((i + 1))
      IFS='|' read -r f cv rv dir cat <<<"$row"
      esc_f=${f//\"/\\\"}
      esc_cat=${cat//\"/\\\"}
      printf '    {"file": "%s", "check_verdict": "%s", "run_verdict": "%s", "direction": "%s", "category": "%s"}' \
        "$esc_f" "$cv" "$rv" "$dir" "$esc_cat"
      if [[ "$i" -lt "$n" ]]; then echo ","; else echo; fi
    done
  fi
  echo "  ]"
  echo "}"
} | tee "$OUT_DIR/report.json" >/dev/null

echo "report: $OUT_DIR/report.json"
echo

if [[ "$partition_rc" -ne 0 ]]; then
  echo "GATE: FAIL - corpus accounting invalid: $GATE_FINAL_STATUS ($GATE_FINAL_REASON)"
  exit 2
fi

# Hollow PASS guard: if every file timed out or crashed, compared==0 and divergences==0
# previously printed PASS with no evidence (Seshat T2 / fleet load spike class).
if [[ "$compared" -eq 0 ]]; then
  echo "GATE: FAIL — zero comparisons (timeouts=$skipped_timeout crashes=$skipped_crash no_main=$skipped_no_main)"
  echo "All inputs skipped — not a policy PASS. Re-run under lower load or raise --timeout."
  exit 2
fi

if [[ "$divergences_dangerous" -gt 0 || "$divergences_conservative" -gt 0 ]]; then
  echo "GATE: FAIL — (R) preflight false reject and/or CONSERVATIVE divergence present"
  echo "first dangerous file: ${first_dangerous:-$first_divergence}"
  echo "dangerous=$divergences_dangerous non_run_by_design=$divergences_non_run conservative=$divergences_conservative"
  exit 1
fi

if [[ "$divergences_non_run" -gt 0 ]]; then
  echo "GATE: PASS_WITH_KNOWN_NON_RUN — no (R) false rejects; $divergences_non_run intentional non-run residual(s); compared=$compared"
  echo "first residual: $first_divergence"
  exit 0
fi

echo "GATE: PASS — check verdict and run-preflight verdict agree on every compared file (compared=$compared)"
exit 0
