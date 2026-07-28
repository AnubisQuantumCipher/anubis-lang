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
#   FAIL_CLOSED_OK        check != 0                  the only fully honest cell
#   CHECK_FA_CRASH        check = 0, run PANICS       promise violation + denied verdict
#   CHECK_FA_CLEAN        check = 0, run fails clean  promise violation
#   RUNS                  check = 0, run = 0          ran; correctness not asserted here
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
  "$BIN" check "$T/x.anb" >/dev/null 2>&1; c=$?
  out="$("$BIN" run "$T/x.anb" 2>&1)"; r=$?
  panicked=0; grep -q 'panicked at' <<<"$out" && panicked=1
  if   [[ $c -ne 0 ]]; then cls=FAIL_CLOSED_OK
  elif [[ $r -eq 0 ]]; then cls=RUNS
  elif [[ $panicked -eq 1 ]]; then cls=CHECK_FA_CRASH
  else cls=CHECK_FA_CLEAN; fi
  d="$(grep -oE 'ANUBIS_[A-Z_]+|panicked at [^ ]+' <<<"$out" | head -1)"
  printf '%s\t%s\t%s\t%s\t%s\n' "$n" "$c" "$r" "$cls" "${d:-}" >> "$OUT"
done < "$LIST"
echo "wrote $OUT"
awk -F'\t' 'NR>1{c[$4]++} END{for (k in c) printf "  %-16s %d\n", k, c[k]}' "$OUT"
