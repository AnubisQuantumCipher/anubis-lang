#!/usr/bin/env bash
# Completion Blueprint Phase 3 label-site census gate.
#
# Runs `scripts/lib/phase3_label_census.py` against the tree and diffs the
# (fn, field, writes, reads) tuples against `docs/phase3/label_census.tsv`. A
# newly-appeared writer/reader function, a change in the (writes, reads) shape
# of an existing bucket, or the introduction of a new field kind fails the
# gate.
#
# Declared verdict (seal-scored):
#   PHASE_3_LABEL_CENSUS: PASS
#   PHASE_3_LABEL_CENSUS: FAIL
#
# Usage:
#   bash scripts/run_phase3_label_census.sh
#   bash scripts/run_phase3_label_census.sh --root path/to/tree
#   bash scripts/run_phase3_label_census.sh --update  # rewrite the census file
#
# --update is a MAINTAINER convenience; the CI-invoked path is the bare form,
# which is read-only and fails closed.
#
set -euo pipefail

ROOT="${GITHUB_WORKSPACE:-$(pwd)}"
UPDATE=0
SELF_TEST=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)      ROOT="$2"; shift 2 ;;
    --update)    UPDATE=1; shift ;;
    --self-test) SELF_TEST=1; shift ;;
    -h|--help)
      sed -n '2,25p' "$0"
      exit 0
      ;;
    *)
      echo "PHASE_3_LABEL_CENSUS: FAIL (unknown flag: $1)"
      exit 2
      ;;
  esac
done

ROOT="$(cd "$ROOT" && pwd)"

if [[ "$SELF_TEST" == "1" ]]; then
  # Run the census regression unittest suite. This is the RED guard that
  # keeps a regression in the tool itself (word-boundary drop, first-match
  # undercount, missing --update bootstrap) from silently passing the gate.
  cd "$ROOT"
  if python3 -m unittest -v scripts.test_phase3_label_census 1>&2; then
    echo "PHASE_3_LABEL_CENSUS_SELFTEST: PASS"
    exit 0
  else
    echo "PHASE_3_LABEL_CENSUS_SELFTEST: FAIL"
    exit 1
  fi
fi

CENSUS_TOOL="$ROOT/scripts/lib/phase3_label_census.py"
EXPECT_FILE="$ROOT/docs/phase3/label_census.tsv"

if [[ ! -x "$CENSUS_TOOL" && ! -f "$CENSUS_TOOL" ]]; then
  echo "PHASE_3_LABEL_CENSUS: FAIL (missing tool: $CENSUS_TOOL)"
  exit 2
fi
if [[ "$UPDATE" != "1" && ! -f "$EXPECT_FILE" ]]; then
  echo "PHASE_3_LABEL_CENSUS: FAIL (missing expectation file: $EXPECT_FILE)"
  exit 2
fi

if [[ "$UPDATE" == "1" ]]; then
  # Rewrite EXPECT_FILE preserving classification columns for known fns.
  # Any newly-appeared fn is written with kind=<UNCLASSIFIED> so the maintainer
  # must hand-classify it before landing. Bootstraps when EXPECT_FILE is
  # absent (the Python block tolerates FileNotFoundError).
  mkdir -p "$(dirname "$EXPECT_FILE")"
  python3 - "$ROOT" "$EXPECT_FILE" <<'PY'
import sys, subprocess
root, expect = sys.argv[1], sys.argv[2]
existing = {}
try:
    with open(expect) as fh:
        for i, line in enumerate(fh):
            parts = line.rstrip("\n").split("\t")
            if i == 0 or parts[0] == "__totals__":
                continue
            fn, field = parts[0], parts[1]
            kind = parts[4] if len(parts) > 4 else "<UNCLASSIFIED>"
            slice_ = parts[5] if len(parts) > 5 else "-"
            notes = parts[6] if len(parts) > 6 else ""
            existing[(fn, field)] = (kind, slice_, notes)
except FileNotFoundError:
    pass

r = subprocess.run(
    ["python3", f"{root}/scripts/lib/phase3_label_census.py", "--root", root],
    capture_output=True, text=True, check=True,
)
lines = r.stdout.strip().splitlines()

rows = ["fn\tfield\twrites\treads\tkind\ttarget_slice\tnotes"]
for line in lines:
    if line.startswith("__totals__"):
        continue
    fn, field, w, r_ = line.split("\t")
    kind, slice_, notes = existing.get((fn, field), ("<UNCLASSIFIED>", "-", ""))
    rows.append(f"{fn}\t{field}\t{w}\t{r_}\t{kind}\t{slice_}\t{notes}")
totals = [l for l in lines if l.startswith("__totals__")][0]
rows.append(totals + "\t-\t-\ttotals")
with open(expect, "w") as fh:
    fh.write("\n".join(rows) + "\n")
print(f"wrote {expect} with {len(rows)} rows")
PY
  echo "PHASE_3_LABEL_CENSUS: UPDATED"
  exit 0
fi

CURRENT="$(python3 "$CENSUS_TOOL" --root "$ROOT")"

# Normalise expected: strip TSV header and the classification columns; keep only
# the (fn, field, writes, reads) tuples the tool produces, and the __totals__
# trailer.
EXPECT_NORM="$(awk -F'\t' 'NR==1 { next } { printf "%s\t%s\t%s\t%s\n", $1, $2, $3, $4 }' "$EXPECT_FILE")"

# Compare current census (from source) against the normalised expectation.
DIFF="$(diff -u <(printf '%s\n' "$EXPECT_NORM") <(printf '%s\n' "$CURRENT") || true)"

if [[ -z "$DIFF" ]]; then
  # Additionally verify no <UNCLASSIFIED> rows remain — every enumerated fn must
  # be tagged with its Phase-3 target slice before the gate can pass.
  UNCLASSIFIED_COUNT="$(awk -F'\t' 'NR>1 && $5=="<UNCLASSIFIED>" { c++ } END { print c+0 }' "$EXPECT_FILE")"
  if [[ "$UNCLASSIFIED_COUNT" -gt 0 ]]; then
    echo "unclassified rows in $EXPECT_FILE:"
    awk -F'\t' 'NR>1 && $5=="<UNCLASSIFIED>"' "$EXPECT_FILE"
    echo "PHASE_3_LABEL_CENSUS: FAIL ($UNCLASSIFIED_COUNT unclassified writer/reader)"
    exit 1
  fi
  ROW_COUNT="$(printf '%s\n' "$CURRENT" | grep -cv '^__totals__' || true)"
  TOTALS="$(printf '%s\n' "$CURRENT" | awk -F'\t' '$1=="__totals__" { print $3, $4 }')"
  echo "census rows: $ROW_COUNT ; totals(writes reads): $TOTALS"
  echo "PHASE_3_LABEL_CENSUS: PASS"
  exit 0
fi

echo "label-site census drifted from $EXPECT_FILE"
echo "--- expected"
echo "+++ current"
printf '%s\n' "$DIFF"
echo
echo "If the drift is intentional, hand-classify any new (fn, field) rows and"
echo "regenerate the expectation with: bash scripts/run_phase3_label_census.sh --update"
echo
echo "PHASE_3_LABEL_CENSUS: FAIL"
exit 1
