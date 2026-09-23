#!/usr/bin/env bash
# Soundness acceptance matrix runner.
#
#   run.sh <anubis-binary> [--record <label> <source-commit>]
#
# Grades every case in registry.tsv (each listed form) against its INTENDED semantics:
#   REJECT     a reachable violation must be refused: DISPROVED, MIXED, or a security refusal
#   ACCEPT     must check clean (rc 0)
#   UNRES      must be an explicit ANUBIS_ASSERTION_UNDECIDED (never rc 0)
#   REJ|UNRES  any refusal except an invalid-input error (never rc 0)
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
    *) echo "INVALID:${code:-rc$rc}" ;;
  esac
}
meets() { # intent class
  case "$1" in
    ACCEPT) [[ $2 == ACCEPT ]] ;;
    REJECT) [[ $2 == DISPROVED || $2 == MIXED || $2 == SEC_REJECT ]] ;;
    UNRES) [[ $2 == UNDECIDED ]] ;;
    'REJ|UNRES') [[ $2 == DISPROVED || $2 == MIXED || $2 == SEC_REJECT || $2 == UNDECIDED ]] ;;
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
