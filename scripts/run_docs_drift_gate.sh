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

OUT_DIR="out/docs_drift_gate"
SCAN_ROOT="$ROOT"
SELF_TEST=0
DERIVED_OVERRIDE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out) OUT_DIR="$2"; shift 2 ;;
    --scan-root) SCAN_ROOT="$(cd "$2" && pwd)"; shift 2 ;;
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

mkdir -p "$OUT_DIR"
REPORT_TXT="$OUT_DIR/docs_drift_report.txt"
DERIVE_PY="$ROOT/scripts/lib/docs_drift_derive.py"
SCAN_PY="$ROOT/scripts/lib/docs_drift_scan.py"

if [[ ! -f "$DERIVE_PY" || ! -f "$SCAN_PY" ]]; then
  echo "DOCS_DRIFT_GATE: FAIL"
  echo "missing derive/scan helpers under scripts/lib/" >&2
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
import json
d=json.load(open("'"$OUT_DIR"'/derived.json"))
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
')"

{
  echo "docs_drift_gate"
  echo "root=$ROOT"
  echo "scan_root=$SCAN_ROOT"
  echo "security_fixtures=$M_SECURITY  # find examples/security -name '*.anb' | wc -l"
  echo "language_fixtures=$M_LANGUAGE  # find tests/fixtures/language_core -name '*.anb' | wc -l"
  echo "stdlib_failclosed=$M_STDLIB  # ls tests/fixtures/stdlib/*should_fail_closed.anb | wc -l"
  echo "stdlib_doc_ok=$M_DOC_OK  # ls tests/fixtures/stdlib/doc_ok/*.anb | wc -l"
  echo "stdlib_modules=$M_MODULES  # ls compiler/stdlib/std/ | wc -l"
  echo "native_corpus=$M_NATIVE  # find examples tests/fixtures -name '*.anb' | wc -l"
  echo "builtins=$M_BUILTINS  # LIVE five-function union in run.rs (no cache file)"
  echo "lean_theorems=$M_LEAN_TH  # comment-stripped ^\\\\s*theorem "
  echo "lean_modules=$M_LEAN_MOD  # modules with ≥1 theorem"
} | tee -a "$REPORT_TXT"

# ── 2. Scan live docs ────────────────────────────────────────────────────────
set +e
python3 "$SCAN_PY" "$SCAN_ROOT" "$OUT_DIR/derived.json" >"$OUT_DIR/scan.json"
SCAN_RC=$?
set -e

STAMPS_CHECKED="$(python3 -c 'import json; print(json.load(open("'"$OUT_DIR"'/scan.json"))["stamps_checked"])')"
CLAIM_GUARDS_CHECKED="$(python3 -c 'import json; print(json.load(open("'"$OUT_DIR"'/scan.json"))["claim_guards_checked"])')"
SCAN_FAILS="$(python3 -c 'import json; print(json.load(open("'"$OUT_DIR"'/scan.json"))["scan_failures"])')"
python3 -c '
import json
d=json.load(open("'"$OUT_DIR"'/scan.json"))
open("'"$OUT_DIR"'/scan_failures.log","w").write("\n".join(d["failures"])+("\n" if d["failures"] else ""))
'
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
    fc="$(python3 -c 'import json; print(json.load(open("'"$tdir"'/guards/fail_'"$name"'.json"))["scan_failures"])')"
    fails="$(python3 -c 'import json; print("\n".join(json.load(open("'"$tdir"'/guards/fail_'"$name"'.json"))["failures"]))')"
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
  fc="$(python3 -c 'import json; print(json.load(open("'"$tdir"'/guards/drift_fail.json"))["scan_failures"])')"
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
  fc="$(python3 -c 'import json; print(json.load(open("'"$tdir"'/guards/drift_pass.json"))["scan_failures"])')"
  if [[ "$fc" -ne 0 ]]; then
    echo "SELFTEST FAIL: drift_pass not clean ($fc)" | tee -a "$REPORT_TXT"
    python3 -c 'import json; print("\n".join(json.load(open("'"$tdir"'/guards/drift_pass.json"))["failures"]))' | tee -a "$REPORT_TXT"
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
  fc="$(python3 -c 'import json; print(json.load(open("'"$tdir"'/guards/dated_pass.json"))["scan_failures"])')"
  if [[ "$fc" -ne 0 ]]; then
    echo "SELFTEST FAIL: dated_pass not clean ($fc)" | tee -a "$REPORT_TXT"
    python3 -c 'import json; print("\n".join(json.load(open("'"$tdir"'/guards/dated_pass.json"))["failures"]))' | tee -a "$REPORT_TXT"
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
  fc="$(python3 -c 'import json; print(json.load(open("'"$tdir"'/guards/ban_pass.json"))["scan_failures"])')"
  if [[ "$fc" -ne 0 ]]; then
    echo "SELFTEST FAIL: ban_pass not clean ($fc)" | tee -a "$REPORT_TXT"
    python3 -c 'import json; print("\n".join(json.load(open("'"$tdir"'/guards/ban_pass.json"))["failures"]))' | tee -a "$REPORT_TXT"
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

if [[ "$FAILS" -eq 0 ]]; then
  echo "DOCS_DRIFT_GATE: PASS"
  echo "Overall: PASS ($STAMPS_CHECKED stamps checked, 0 drift)"
  exit 0
fi
echo "DOCS_DRIFT_GATE: FAIL"
echo "Overall: FAIL (scan_failures=$FAILS)"
exit 1
