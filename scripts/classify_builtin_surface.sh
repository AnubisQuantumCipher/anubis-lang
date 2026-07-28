#!/usr/bin/env bash
set -uo pipefail
# Classify every builtin cell by what `check` and `run` DO, not by what the docs say.
#
# The column that matters is the PAIR. A cell where `check` passes and `run` fails is a promise
# violation regardless of how cleanly `run` fails — the promise sentence says a PASS means no way
# was found for the program to violate its contracts, so a program that dies at runtime is one
# `check` should have rejected. A run-only classification cannot express that, which is why the
# first pass at this matrix could not see the six HOF builtins that accepted-then-panicked.
#
# The first version of this header asserted a STRONGER promise than the one this repo publishes:
# "a program that dies at runtime is one `check` should have rejected". The published sentence is
#
#     check PASS => Anubis found no way for the program to violate its CONTRACTS, EFFECTS,
#     CAPABILITIES or INFORMATION-FLOW -- and everything it could not decide, it REFUSED rather
#     than assumed
#
# Totality is not on that list. A run that stops with a structured `ANUBIS_*` refusal did exactly
# what the second clause promises: it refused. Grading those as promise violations put 13
# fail-closed refusals in a violation bucket, and the pressure that creates is to weaken the
# runtime until the board looks green -- the opposite of the intent.
#
# So the pair is graded by WHAT THE RUN DID, and a refusal is distinguished from a break. Nothing is
# forgiven: panics and unstructured failures still count against the surface.
#
#   FAIL_CLOSED_OK        check != 0                    check refused; the fully honest cell
#   CHECK_FA_CRASH        check = 0, run PANICS         violation + denied verdict
#   RUN_FAILS_UNSTRUCTURED check = 0, run != 0, no code violation: failed in an unnamed shape
#   RUN_REFUSES           check = 0, run != 0 + ANUBIS_ runtime refused; promise HOLDS, but
#                                                       `check` is INCOMPLETE about runnability
#   RUNS                  check = 0, run = 0            ran; correctness not asserted here
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
BIN="${ANUBIS_BIN:-$(bash scripts/publish_pin.sh --current 2>/dev/null)}"
[[ -x "$BIN" ]] || { echo "no usable binary" >&2; exit 2; }
LIST="${1:?usage: classify_builtin_surface.sh NAMES_FILE [OUT_TSV]}"
OUT="${2:-scratchpad/fleet_20260726/w19/p5/surface.tsv}"
mkdir -p "$(dirname "$OUT")"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
printf 'name\tcheck\trun\tclass\tdetail\n' > "$OUT"
while read -r n; do
  [[ -n "$n" ]] || continue
  # Wrong-ARITY probe: call with zero arguments. Every builtin that takes any is mis-called, and a
  # correct implementation must refuse rather than panic.
  printf 'fn main() { %s(); }\n' "$n" > "$T/x.anb"
  # </dev/null is LOAD-BEARING: without it these inherit the loop's stdin and consume names
  # straight out of "$LIST". A 213-name list silently measured 92 rows that way, and the missing
  # 121 did not appear as failures -- they appeared as if they had never been asked about, which is
  # the single most dangerous shape a measurement can take.
  "$BIN" check "$T/x.anb" >/dev/null 2>&1 </dev/null; c=$?
  out="$("$BIN" run "$T/x.anb" 2>&1 </dev/null)"; r=$?
  panicked=0; grep -q 'panicked at' <<<"$out" && panicked=1
  refused=0; grep -qE 'ANUBIS_[A-Z_]+' <<<"$out" && refused=1
  if   [[ $c -ne 0 ]]; then cls=FAIL_CLOSED_OK
  elif [[ $r -eq 0 ]]; then cls=RUNS
  elif [[ $panicked -eq 1 ]]; then cls=CHECK_FA_CRASH
  elif [[ $refused -eq 1 ]]; then cls=RUN_REFUSES
  else cls=RUN_FAILS_UNSTRUCTURED; fi
  d="$(grep -oE 'ANUBIS_[A-Z_]+|panicked at [^ ]+' <<<"$out" | head -1)"
  printf '%s\t%s\t%s\t%s\t%s\n' "$n" "$c" "$r" "$cls" "${d:-}" >> "$OUT"
done < "$LIST"
# Row-count conservation: the instrument must answer about every name it was handed.
want=$(grep -cvE '^\s*$' "$LIST"); got=$(( $(wc -l < "$OUT") - 1 ))
if [[ "$want" -ne "$got" ]]; then
  echo "CLASSIFY: FAIL (asked about $want names, reported $got -- the loop dropped rows)" >&2
  exit 3
fi
echo "wrote $OUT ($got/$want names measured)"
awk -F'\t' 'NR>1{c[$4]++} END{for (k in c) printf "  %-16s %d\n", k, c[k]}' "$OUT"
