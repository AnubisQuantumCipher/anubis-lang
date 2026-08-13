#!/usr/bin/env bash
# Docs drift gate — fail-closed inventory + live stamp + absolute-phrase checks.
#
# Re-derives every live quantitative claim BY COMMAND (scripts/lib/docs_drift_derive.py),
# scans owned live docs for undated present-tense stamps that disagree
# (scripts/lib/docs_drift_scan.py), and rejects absolute unfalsifiable phrasings
# unless residual/meta context is present.
#
# Dated / historical seal framing does NOT fail the gate.
#
# Usage:
#   bash scripts/run_docs_drift_gate.sh
#   bash scripts/run_docs_drift_gate.sh --out out/docs_drift
#   bash scripts/run_docs_drift_gate.sh --scan-root path/to/fixture_docs
#   bash scripts/run_docs_drift_gate.sh --derived-json path/to/derived.json  # tests only
#   bash scripts/run_docs_drift_gate.sh --self-test
#
# Declared verdict (seal-scored):
#   DOCS_DRIFT_GATE: PASS
#   DOCS_DRIFT_GATE: FAIL
#
# Bash 3.2 compatible. Does not need ANUBIS_BIN (tree inventory only).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"

OUT_DIR=""
SCAN_ROOT="$ROOT"
SELF_TEST=0
DERIVED_OVERRIDE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT_DIR="$2"; shift 2 ;;
    --scan-root) SCAN_ROOT="$(cd "$2" && pwd -P)"; shift 2 ;;
    --derived-json) DERIVED_OVERRIDE="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"; shift 2 ;;
    --self-test) SELF_TEST=1; shift ;;
    -h|--help)
      sed -n '2,28p' "$0"
      exit 0
      ;;
    *)
      echo "unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -n "$DERIVED_OVERRIDE" && "$SELF_TEST" -ne 1 ]]; then
  echo "--derived-json is test-only and requires --self-test" >&2
  exit 2
fi

if [[ -n "$OUT_DIR" ]]; then
  mkdir -p "$OUT_DIR"
else
  mkdir -p "$ROOT/out"
  OUT_DIR="$(mktemp -d "$ROOT/out/docs_drift_gate.XXXXXX")"
fi
OUT_LOCK="$OUT_DIR/.anubis-docs-drift.lock"
if ! mkdir "$OUT_LOCK" 2>/dev/null; then
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "output directory is already in use: $OUT_DIR" >&2
  exit 2
fi
trap 'rmdir "$OUT_LOCK" 2>/dev/null || true' EXIT
if ! assert_clean_output_dir "$OUT_DIR" ".anubis-docs-drift.lock" "docs drift gate"; then
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "$GATE_OUTPUT_DIR_ERROR" >&2
  exit 2
fi
echo "docs_drift_out=$OUT_DIR"
REPORT_TXT="$OUT_DIR/docs_drift_report.txt"
DERIVE_PY="$ROOT/scripts/lib/docs_drift_derive.py"
SCAN_PY="$ROOT/scripts/lib/docs_drift_scan.py"

json_field() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"
}
json_failures() {
  python3 -c 'import json,sys; print("\n".join(json.load(open(sys.argv[1]))["failures"]))' "$1"
}
json_write_failures() {
  python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); open(sys.argv[2],"w").write("\n".join(d["failures"])+("\n" if d["failures"] else ""))' "$1" "$2"
}

if [[ ! -f "$DERIVE_PY" || ! -f "$SCAN_PY" ]]; then
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "missing derive/scan helpers under scripts/lib/" >&2
  exit 1
fi

if ! python3 "$ROOT/scripts/test_docs_drift_scan.py" >"$OUT_DIR/scanner_unit.log" 2>&1; then
  cat "$OUT_DIR/scanner_unit.log" >&2
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "docs drift scanner unit tests failed" >&2
  exit 1
fi

: >"$REPORT_TXT"

# ── 1. Re-derive ─────────────────────────────────────────────────────────────
if [[ -n "$DERIVED_OVERRIDE" ]]; then
  if [[ ! -f "$DERIVED_OVERRIDE" ]]; then
    echo "DOCS_DRIFT_GATE: FAIL"
    echo "missing --derived-json input: $DERIVED_OVERRIDE" >&2
    exit 1
  fi
  cp "$DERIVED_OVERRIDE" "$OUT_DIR/derived.json"
else
  python3 "$DERIVE_PY" "$ROOT" >"$OUT_DIR/derived.json"
fi

eval "$(python3 -c '
import json, sys
d=json.load(open(sys.argv[1]))
q=d["quantities"]
print("M_SECURITY=%d" % q["security_fixtures"]["value"])
print("M_LANGUAGE=%d" % q["language_fixtures"]["value"])
print("M_STDLIB=%d" % q["stdlib_failclosed"]["value"])
print("M_DOC_OK=%d" % q["stdlib_doc_ok"]["value"])
print("M_MODULES=%d" % q["stdlib_modules"]["value"])
print("M_NATIVE=%d" % q["native_corpus"]["value"])
print("M_BUILTINS=%d" % q["builtins"]["value"])
print("M_LEAN_TH=%d" % d["lean"]["theorems"])
print("M_LEAN_MOD=%d" % d["lean"]["modules"])
' "$OUT_DIR/derived.json")"

{
  echo "docs_drift_gate"
  echo "root=$ROOT"
  echo "scan_root=$SCAN_ROOT"
  echo "security_fixtures=$M_SECURITY  # find examples/security -name '*.anb' | wc -l"
  echo "language_fixtures=$M_LANGUAGE  # find tests/fixtures/language_core -name '*.anb' | wc -l"
  echo "stdlib_failclosed=$M_STDLIB  # ls tests/fixtures/stdlib/*should_fail_closed.anb | wc -l"
  echo "stdlib_doc_ok=$M_DOC_OK  # ls tests/fixtures/stdlib/doc_ok/*.anb | wc -l"
  echo "stdlib_modules=$M_MODULES  # ls compiler/stdlib/std/ | wc -l"
  echo "native_corpus=$M_NATIVE  # git ls-files examples/**/*.anb tests/fixtures/**/*.anb | wc -l"
  echo "builtins=$M_BUILTINS  # LIVE five-function union in run.rs (no cache file)"
  echo "lean_theorems=$M_LEAN_TH  # comment-stripped ^\\\\s*theorem "
  echo "lean_modules=$M_LEAN_MOD  # modules with ≥1 theorem"
} | tee -a "$REPORT_TXT"

# ── 2. Scan live docs ────────────────────────────────────────────────────────
SCAN_ARGS=("$SCAN_ROOT" "$OUT_DIR/derived.json")
if [[ "$SCAN_ROOT" == "$ROOT" ]]; then
  # Fixture roots are intentionally sparse; the canonical owned-doc inventory is not.
  # A rename or deletion in the live tree must be a finding, never a silent `continue`.
  SCAN_ARGS+=(--require-owned-files)
fi
set +e
python3 "$SCAN_PY" "${SCAN_ARGS[@]}" >"$OUT_DIR/scan.json"
SCAN_RC=$?
set -e

# Test-only poison can only remove/corrupt scanner output, exercising fail-closed parsing.
case "${ANUBIS_TEST_ONLY_DOCS_DRIFT_SCAN_POISON:-}" in
  "") ;;
  missing) rm -f "$OUT_DIR/scan.json" ;;
  invalid) printf '{not-json\n' >"$OUT_DIR/scan.json" ;;
  *) echo "DOCS_DRIFT_GATE: FAIL (unknown scan poison)" >&2; exit 2 ;;
esac

STAMPS_CHECKED="$(json_field "$OUT_DIR/scan.json" stamps_checked)"
CLAIM_GUARDS_CHECKED="$(json_field "$OUT_DIR/scan.json" claim_guards_checked)"
SCAN_FAILS="$(json_field "$OUT_DIR/scan.json" scan_failures)"
json_write_failures "$OUT_DIR/scan.json" "$OUT_DIR/scan_failures.log"
echo "stamps_checked=$STAMPS_CHECKED claim_guards_checked=$CLAIM_GUARDS_CHECKED scan_fails=$SCAN_FAILS" | tee -a "$REPORT_TXT"
if [[ "$SCAN_FAILS" -gt 0 ]]; then
  echo "---- scan failures ----" | tee -a "$REPORT_TXT"
  cat "$OUT_DIR/scan_failures.log" | tee -a "$REPORT_TXT" || true
fi

# ── 3. Self-test microbenches ────────────────────────────────────────────────
SELF_RC=0
run_self_test() {
  local tdir="$OUT_DIR/selftest"
  rm -rf "$tdir"
  mkdir -p "$tdir/guards"

  local wrong_sec=$((M_SECURITY - 1))
  local wrong_lang=$((M_LANGUAGE - 1))
  local wrong_std=$((M_STDLIB - 1))
  local wrong_nat=$((M_NATIVE - 1))
  local wrong_bi=$((M_BUILTINS - 1))
  local wrong_th=$((M_LEAN_TH - 1))
  local wrong_mod=$((M_LEAN_MOD - 1))
  local wrong_dok=$((M_DOC_OK - 1))
  local wrong_smod=$((M_MODULES - 1))
  [[ $wrong_sec -lt 1 ]] && wrong_sec=1
  [[ $wrong_lang -lt 1 ]] && wrong_lang=1
  [[ $wrong_std -lt 1 ]] && wrong_std=1
  [[ $wrong_nat -lt 1 ]] && wrong_nat=1
  [[ $wrong_bi -lt 100 ]] && wrong_bi=150
  [[ $wrong_th -lt 1 ]] && wrong_th=1
  [[ $wrong_mod -lt 1 ]] && wrong_mod=1
  [[ $wrong_dok -lt 1 ]] && wrong_dok=1
  [[ $wrong_smod -lt 1 ]] && wrong_smod=1

  # Per-quantity FAIL microbenches (plan criterion 2: every live quantity guard must fire)
  _qty_fail() {
    local name="$1" body="$2" needle="$3"
    local d="$tdir/fail_$name"
    mkdir -p "$d"
    printf '%s\n' "$body" >"$d/AGENTS.md"
    set +e
    python3 "$SCAN_PY" "$d" "$OUT_DIR/derived.json" >"$tdir/guards/fail_${name}.json"
    set -e
    local fc fails
    fc="$(json_field "$tdir/guards/fail_${name}.json" scan_failures)"
    fails="$(json_failures "$tdir/guards/fail_${name}.json")"
    if [[ "$fc" -lt 1 ]] || ! echo "$fails" | grep -q "$needle"; then
      echo "SELFTEST FAIL: fail_$name did not fire needle=$needle fc=$fc fails=$fails" | tee -a "$REPORT_TXT"
      return 1
    fi
    echo "SELFTEST PASS: fail_$name fired ($needle)" | tee -a "$REPORT_TXT"
  }

  _qty_fail security \
    "## Current state"$'\n'"security **${wrong_sec}/${wrong_sec}**" \
    "security claimed" || return 1
  _qty_fail language \
    "## Current state"$'\n'"language **${wrong_lang}/${wrong_lang}**" \
    "language claimed" || return 1
  _qty_fail stdlib \
    "## Current state"$'\n'"stdlib fail-closed **${wrong_std}/${wrong_std}**" \
    "stdlib_failclosed claimed" || return 1
  _qty_fail native \
    "## Current state"$'\n'"native-authoritative **${wrong_nat} files, 0 mismatches**." \
    "native_corpus claimed" || return 1
  _qty_fail builtins \
    "## Current state"$'\n'"Builtins are ${wrong_bi}." \
    "builtins claimed" || return 1
  _qty_fail lean \
    "## Current state"$'\n'"Lean is ${wrong_th} theorems across ${wrong_mod} modules." \
    "lean_" || return 1
  _qty_fail doc_ok \
    "## Current state"$'\n'"DOC_OK locks under tests/fixtures/stdlib/doc_ok/ (${wrong_dok} fixtures)." \
    "stdlib_doc_ok claimed" || return 1
  _qty_fail modules \
    "## Current state"$'\n'"Standard library: ${wrong_smod} content-locked Anubis-source modules (compiler/stdlib/std/)." \
    "stdlib_modules claimed" || return 1

  # Combined drift_fail (security still covered)
  mkdir -p "$tdir/drift_fail"
  cat >"$tdir/drift_fail/AGENTS.md" <<EOF
## Current state (2026-07-27)
security **${wrong_sec}/${wrong_sec}** · language **${M_LANGUAGE}/${M_LANGUAGE}** · stdlib fail-closed **${M_STDLIB}/${M_STDLIB}**
EOF
  set +e
  python3 "$SCAN_PY" "$tdir/drift_fail" "$OUT_DIR/derived.json" >"$tdir/guards/drift_fail.json"
  set -e
  local fc
  fc="$(json_field "$tdir/guards/drift_fail.json" scan_failures)"
  if [[ "$fc" -lt 1 ]]; then
    echo "SELFTEST FAIL: drift_fail did not fire" | tee -a "$REPORT_TXT"
    return 1
  fi
  echo "SELFTEST PASS: drift_fail fired (count=$fc)" | tee -a "$REPORT_TXT"

  # drift_pass — all quantities correct
  mkdir -p "$tdir/drift_pass"
  cat >"$tdir/drift_pass/AGENTS.md" <<EOF
## Current state (2026-07-27)
security **${M_SECURITY}/${M_SECURITY}** · language **${M_LANGUAGE}/${M_LANGUAGE}** · stdlib fail-closed **${M_STDLIB}/${M_STDLIB}**
native-authoritative **${M_NATIVE} files, 0 mismatches**.
Builtins are ${M_BUILTINS}.
Lean is ${M_LEAN_TH} theorems across ${M_LEAN_MOD} modules.
Complete inventory (${M_BUILTINS} builtins).
**Count: ${M_BUILTINS}**
DOC_OK locks under tests/fixtures/stdlib/doc_ok/ (${M_DOC_OK} fixtures).
Standard library: ${M_MODULES} content-locked Anubis-source modules (compiler/stdlib/std/).
EOF
  set +e
  python3 "$SCAN_PY" "$tdir/drift_pass" "$OUT_DIR/derived.json" >"$tdir/guards/drift_pass.json"
  set -e
  fc="$(json_field "$tdir/guards/drift_pass.json" scan_failures)"
  if [[ "$fc" -ne 0 ]]; then
    echo "SELFTEST FAIL: drift_pass not clean ($fc)" | tee -a "$REPORT_TXT"
    json_failures "$tdir/guards/drift_pass.json" | tee -a "$REPORT_TXT"
    return 1
  fi
  echo "SELFTEST PASS: drift_pass clean" | tee -a "$REPORT_TXT"

  # dated_pass
  mkdir -p "$tdir/dated_pass/docs"
  cat >"$tdir/dated_pass/AGENTS.md" <<'EOF'
## Historical
As of the 2026-07-24 seal, security **228/228** and stdlib **45/45** (snapshot only — not current).
EOF
  cat >"$tdir/dated_pass/docs/CLAIMS.md" <<'EOF'
| Surface | Observation |
|---|---|
| Security | As of seal 2026-07-24: **228/228 PASS** (historical) |
| Native | **CLAIMED 2026-07-25** PASS over 681 files (dated claim) |
EOF
  set +e
  python3 "$SCAN_PY" "$tdir/dated_pass" "$OUT_DIR/derived.json" >"$tdir/guards/dated_pass.json"
  set -e
  fc="$(json_field "$tdir/guards/dated_pass.json" scan_failures)"
  if [[ "$fc" -ne 0 ]]; then
    echo "SELFTEST FAIL: dated_pass not clean ($fc)" | tee -a "$REPORT_TXT"
    json_failures "$tdir/guards/dated_pass.json" | tee -a "$REPORT_TXT"
    return 1
  fi
  echo "SELFTEST PASS: dated_pass ignores historical N" | tee -a "$REPORT_TXT"

  # One deliberate FAIL per semantic-claim guard.
  _qty_fail claim_check_run \
    'A green `anubis check` never certifies a contract that `anubis run` violates.' \
    "check-run-invariant" || return 1
  _qty_fail claim_absolute_promise \
    'The program cannot violate its stated contracts, effects, capabilities, or information-flow policy at runtime.' \
    "absolute-check-promise" || return 1
  _qty_fail claim_privacy \
    'The info-flow lane guarantees nothing private leaves.' \
    "privacy-absolute" || return 1
  _qty_fail claim_everywhere \
    'Safe fails closed, everywhere.' \
    "fails-closed-everywhere" || return 1
  _qty_fail claim_totality \
    'Safe-mode is total IFC.' \
    "totality-finality" || return 1
  _qty_fail claim_aggregate \
    'Every guarantee is proven-or-scoped.' \
    "aggregate-proof" || return 1
  _qty_fail claim_walker_count \
    'There are ~19 independent value-flow walkers.' \
    "approximate-walker-count" || return 1
  _qty_fail claim_seal \
    'The self-host fixpoint is sealed.' \
    "sealed-without-evidence-path" || return 1

  # ban_pass
  mkdir -p "$tdir/ban_pass"
  cat >"$tdir/ban_pass/AGENTS.md" <<'EOF'
It fails closed by design and by default — and where it does not yet, the gap is published
rather than papered over: see docs/CLAIMS.md bounded residual. Green means no KNOWN defects, not no defects.
Do not stamp "false-accept class closed forever," "roadmap soundness complete," or "Safe is total IFC".
EOF
  set +e
  python3 "$SCAN_PY" "$tdir/ban_pass" "$OUT_DIR/derived.json" >"$tdir/guards/ban_pass.json"
  set -e
  fc="$(json_field "$tdir/guards/ban_pass.json" scan_failures)"
  if [[ "$fc" -ne 0 ]]; then
    echo "SELFTEST FAIL: ban_pass not clean ($fc)" | tee -a "$REPORT_TXT"
    json_failures "$tdir/guards/ban_pass.json" | tee -a "$REPORT_TXT"
    return 1
  fi
  echo "SELFTEST PASS: ban_pass residual-linked / meta banlist" | tee -a "$REPORT_TXT"

  echo "SELFTEST: all guards demonstrated" | tee -a "$REPORT_TXT"
  return 0
}

FAILS="$SCAN_FAILS"
if [[ "$SELF_TEST" -eq 1 ]]; then
  if ! run_self_test; then
    SELF_RC=1
    FAILS=$((FAILS + 1))
  else
    SELF_RC=0
  fi
else
  SELF_RC=0
fi

write_report() {
python3 - "$OUT_DIR" "$FAILS" "$STAMPS_CHECKED" "$SELF_TEST" "$SELF_RC" <<'PY'
import json, sys
from pathlib import Path
out, fails, stamps, self_test, self_rc = sys.argv[1:6]
derived = json.loads(Path(out, "derived.json").read_text())
scan = json.loads(Path(out, "scan.json").read_text())
report = {
    "gate": "docs_drift",
    "overall_verdict": "PASS" if int(fails) == 0 else "FAIL",
    "scan_failures": int(fails),
    "stamps_checked": int(stamps),
    "self_test_requested": self_test == "1",
    "self_test_ok": self_rc == "0",
    "derived": derived,
    "scan": scan,
    "failures": scan.get("failures", []),
}
Path(out, "docs_drift_report.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
PY
}

# Coverage is part of the verdict. This gate printed
#   DOCS_DRIFT_GATE: PASS / Overall: PASS (0 stamps checked, 0 drift)
# with exit 0 against an empty scan root — demonstrated by counterexample 2026-07-28 — because the
# verdict below reads only $FAILS and $STAMPS_CHECKED was decorative. A rename of an owned doc
# produced the same vacuous green, and the seal consumes this gate by matching its PASS token.
if ! assert_tested "$STAMPS_CHECKED" "stamps_checked" "$CLAIM_GUARDS_CHECKED" "claim_guards_checked"; then
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "Overall: FAIL (vacuous: $GATE_COVERAGE_ERROR)" >&2
  exit 1
fi

if [[ "$SCAN_RC" -gt 1 \
  || ( "$SCAN_RC" -eq 1 && "$SCAN_FAILS" -eq 0 ) \
  || ( "$SCAN_RC" -eq 0 && "$SCAN_FAILS" -ne 0 ) ]]; then
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "Overall: FAIL (inconsistent scanner result: exit=$SCAN_RC failures=$SCAN_FAILS)" >&2
  exit 1
fi

if [[ "$FAILS" -ne 0 ]]; then
  write_report
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "Overall: FAIL (scan_failures=$FAILS)"
  exit 1
fi

# Coverage RATCHET. `assert_tested` catches a gate that tests NOTHING; it cannot catch one that
# quietly tests LESS.
#
# Demonstrated on this gate 2026-07-28: adding two exemptions to the scanner took it from 42 stamps
# to 30 — a 29% loss of coverage — and it reported `PASS (30 stamps checked, 0 drift)` with no
# indication anything had changed. Every exemption was justified on review, and that is exactly the
# problem: the justified case and the careless case produce identical output. An exemption is the
# one edit that makes a gate greener by making it check less, so it is the one edit that must not be
# silent.
#
# The floor lives in a tracked file. Ordinary verification is read-only; reviewed maintenance may
# raise it with ANUBIS_GATE_UPDATE_FLOORS=1. Lowering requires editing that file in a visible commit.
FLOOR_FILE="$ROOT/docs/.docs_drift_coverage_floor"
if ! assert_floor docs_drift_stamps "$STAMPS_CHECKED" "$FLOOR_FILE"; then
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "Overall: FAIL ($GATE_FLOOR_ERROR)" >&2
  exit 1
fi

write_report
echo "DOCS_DRIFT_GATE: PASS"
echo "Overall: PASS ($STAMPS_CHECKED stamps checked, 0 drift)"
