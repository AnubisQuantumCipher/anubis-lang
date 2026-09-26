#!/usr/bin/env bash
# Soundness acceptance matrix runner.
#
#   run.sh <anubis-binary> [--record <label> <source-commit>]
#
# Grades every case in registry.tsv (each listed form) against its INTENDED semantics:
#   REJECT     a reachable violation must be refused: DISPROVED, MIXED, or a security refusal
#   ACCEPT     must check clean (rc 0)
#   UNRES      must be an explicit ANUBIS_ASSERTION_UNDECIDED (never rc 0)
#   REJ|UNRES  any refusal except an invalid-input error (never rc 0), including REFUSED
#
# Class REFUSED is a named VERIFICATION refusal the checker raises instead of a verdict: it could not
# model a float contract (ANUBIS_FLOAT_CONTRACT_UNMODELED), verify a loop invariant inductively
# (ANUBIS_LOOP_INVARIANT_UNVERIFIABLE), or prove a divisor non-zero (ANUBIS_DIVISOR_MAYBE_ZERO). Each is
# fail-closed (never rc 0) and none is an input error, so each meets only REJ|UNRES: none is a disproof
# (REJECT) nor the assertion-level UNDECIDED that UNRES requires. Only these listed codes qualify.
#   MALFORMED  syntactically invalid: an ordinary diagnostic exit (1); a crash or silent pass fails
#   LIMIT      valid syntax over a documented implementation limit: same required outcome as MALFORMED
# An invalid-input / tool error (parse, type, unknown function, panic) is NEVER counted as a
# rejection: a parser error is not a security result.
#
# --record appends one row per (case, form) to history.tsv: the history is append-only and is the
# durable record of how each case behaved on each source-bound binary. Totals always include every
# registry row: moving a case between categories does not remove it.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
BIN=${1:?usage: run.sh <anubis-binary> [--record <label> <source-commit>]}
LABEL=""; SRC=""
if [[ "${2:-}" == "--record" ]]; then LABEL=${3:?label}; SRC=${4:?source commit}; fi
# A grader must not pass by not asking: refuse a missing binary, and prove it runs, before grading.
[[ -f "$BIN" && -x "$BIN" ]] || { echo "run.sh: not an executable file: $BIN" >&2; exit 2; }
"$BIN" --version >/dev/null 2>&1 || { echo "run.sh: binary does not run: $BIN" >&2; exit 2; }
SHA=$(sha256sum "$BIN" | cut -d' ' -f1)
OUT=$(mktemp -d "${TMPDIR:-/tmp}/anubis-matrix.XXXXXX")

classify() { # rc outfile -> class
  local rc=$1 f=$2 code
  [[ $rc -eq 0 ]] && { echo ACCEPT; return; }
  code=$(grep -oE 'ANUBIS_[A-Z0-9_]+' "$f" | head -1)
  case "$code" in
    ANUBIS_ASSERTION_DISPROVED) echo DISPROVED ;;
    ANUBIS_ASSERTION_UNDECIDED) echo UNDECIDED ;;
    ANUBIS_ASSERTION_UNPROVEN) echo MIXED ;;
    ANUBIS_SECRET_EXFILTRATION|ANUBIS_TAINTED_SINK*|ANUBIS_INTERPROC_*|ANUBIS_EFFECT_*|ANUBIS_CAPABILITY_*|ANUBIS_IMPLICIT_FLOW*)
      echo "SEC_REJECT" ;;
    ANUBIS_WRAP_RISK) echo WRAP ;;
    ANUBIS_FLOAT_CONTRACT_UNMODELED|ANUBIS_LOOP_INVARIANT_UNVERIFIABLE|ANUBIS_DIVISOR_MAYBE_ZERO)
      echo REFUSED ;;
    *) echo "INVALID:${code:-rc$rc}" ;;
  esac
}
meets() { # intent class
  case "$1" in
    ACCEPT) [[ $2 == ACCEPT ]] ;;
    REJECT) [[ $2 == DISPROVED || $2 == MIXED || $2 == SEC_REJECT || $2 == WRAP ]] ;;
    UNRES) [[ $2 == UNDECIDED ]] ;;
    'REJ|UNRES') [[ $2 == DISPROVED || $2 == MIXED || $2 == SEC_REJECT || $2 == WRAP || $2 == UNDECIDED \
                  || $2 == REFUSED ]] ;;
    # Malformed source must be refused with an ordinary diagnostic exit (1): a crash (SIGABRT 134,
    # SIGSEGV 139), a timeout, or a missing tool is not a refusal.
    MALFORMED) [[ $2 == INVALID:* && $3 -eq 1 ]] ;;
    # Over a documented implementation limit: same required outcome as MALFORMED (a diagnostic).
    LIMIT) [[ $2 == INVALID:* && $3 -eq 1 ]] ;;
    *) return 1 ;;
  esac
}

printf 'id\tform\tcategory\tintent\tobserved\tresult\n' > "$OUT/results.tsv"
tail -n +2 "$HERE/registry.tsv" | while IFS=$'\t' read -r id fam cat intent forms notes; do
  IFS=',' read -ra fl <<< "$forms"
  for form in "${fl[@]}"; do
    src="$HERE/cases/$id.anb"; [[ $form == direct ]] && src="$HERE/cases/$id.direct.anb"
    "$BIN" check "$src" > "$OUT/$id.$form.out" 2>&1
    rc=$?
    obs=$(classify $rc "$OUT/$id.$form.out")
    # The direct twin is a comparison column, not an oracle; only the primary form is graded.
    if [[ $form == direct ]]; then res=compare
    elif [[ $intent == MALFORMED || $intent == LIMIT ]]; then
      if meets "$intent" "$obs" "$rc"; then res=PASS
      elif [[ $obs == ACCEPT ]]; then res=FAIL-silent-accept
      else res=FAIL-not-a-diagnostic; fi
    elif [[ $obs == INVALID:* ]]; then res=INVALID
    elif meets "$intent" "$obs"; then res=PASS
    elif [[ $obs == ACCEPT ]]; then res=FAIL-silent-accept
    else res=FAIL-wrong-class; fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$form" "$cat" "$intent" "$obs" "$res" >> "$OUT/results.tsv"
    if [[ -n $LABEL ]]; then
      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$id" "$form" "$LABEL" "$SRC" "$SHA" "$obs" "$res" >> "$HERE/history.tsv"
    fi
  done
done
column -t -s $'\t' "$OUT/results.tsv"
echo
python3 - "$OUT/results.tsv" <<'PY'
import sys, collections
rows=[l.rstrip('\n').split('\t') for l in open(sys.argv[1]).read().splitlines()[1:]]
prim=[r for r in rows if r[1]!='direct']
ids=[r[0] for r in prim]; assert len(ids)==len(set(ids)), "duplicate case id"
for cat in sorted({r[2] for r in prim}):
    rs=[r for r in prim if r[2]==cat]; c=collections.Counter(r[5] for r in rs)
    print(f"{cat}: {len(rs)} cases -> "+", ".join(f"{k}={v}" for k,v in sorted(c.items())))
    for k in sorted(c):
        if k!='PASS': print(f"   {k}: "+" ".join(r[0] for r in rs if r[5]==k))
c=collections.Counter(r[5] for r in prim)
print(f"ALL: {len(prim)} cases -> "+", ".join(f"{k}={v}" for k,v in sorted(c.items())))
PY
echo "binary sha256 $SHA${LABEL:+  recorded as $LABEL @ $SRC}"
rm -rf "$OUT"
