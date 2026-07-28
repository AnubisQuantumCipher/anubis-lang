#!/usr/bin/env bash
# Fast, static registry-parity check between the Rust host's builtin recognizer
# (compiler/src/backends/run.rs::is_builtin_name, assembled from emit_builtin_call's match arms plus
# is_proof_input_builtin/is_poc_kit_builtin/is_non_run_builtin) and the self-hosted mirror
# (selfhost/src/anubis_sh.anb::sh_is_known_builtin).
#
# WHY THIS EXISTS: the whole-corpus check (scripts/run_capset_corpus_failclosed.sh) and the curated
# gate (scripts/run_capset_selfhost_gate.sh) both catch this drift authoritatively, but both require
# a release build plus running the self-hosted interpreter (minutes, and the corpus variant can run
# to an hour) — too slow for a contributor to run before every commit, and exactly the kind of gate
# that goes quietly dead when an unrelated policy change breaks its invocation path (this happened:
# the whole self-host gate family was dead on host for a full day, 2026-07-25 21:21 onward, and SEVEN
# builtin names landed in run.rs during that exact window with nobody the wiser).
#
# This script is the fast, always-on backstop: pure text extraction, ZERO compilation, ZERO
# `anubis run` invocation of any kind — so it cannot be silently disabled by isolation/VZ policy
# changes the way the self-host gate family was. It runs in well under a second and is meant to be
# cheap enough that a contributor (or CI) always runs it, never skips it.
#
# Exit 0  = the self-hosted registry is a subset of the Rust registry (the only safe relationship;
#           a name present in both, or missing from the SH side, is fine — see run_capset_corpus_
#           failclosed.sh's CONSERVATIVE bucket for why "SH doesn't recognize it yet" is safe).
# Exit 1  = either (a) new names exist in run.rs that the SH registry doesn't recognize (drift —
#           the exact class this script exists to catch fast), or (b) the SH registry claims a name
#           that ISN'T in run.rs at all (a correctness bug in the mirror, not just staleness).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

RUN_RS="compiler/src/backends/run.rs"
SH_ANB="selfhost/src/anubis_sh.anb"

python3 - "$RUN_RS" "$SH_ANB" <<'PY'
import re
import sys

run_rs_path, sh_anb_path = sys.argv[1], sys.argv[2]

with open(run_rs_path) as f:
    run_src = f.read()

def block(src, start_marker, end_marker):
    """Slice src from the line containing start_marker to the line containing end_marker
    (both matched as plain substrings, first occurrence after the previous cursor)."""
    start = src.index(start_marker)
    end = src.index(end_marker, start)
    return src[start:end]

# emit_builtin_call: match-arm PATTERN strings only (immediately before `=>`, allowing a bare `if`
# guard with no quotes in between) -- excludes the internal `fixed("anubis_x", ...)` codegen target
# names that appear in arm BODIES, which must never be mistaken for builtin surface names.
emit_block = block(run_src, "fn emit_builtin_call(callee: &str, args: &[String])", "\n}\n")
pattern_re = re.compile(r'((?:"[A-Za-z_][A-Za-z0-9_]*"\s*\|?\s*)+)(?:if\s+[^"=]*?)?=>', re.S)
emit_names = set()
for m in pattern_re.finditer(emit_block):
    emit_names.update(re.findall(r'"([A-Za-z_][A-Za-z0-9_]*)"', m.group(1)))

def names_in(src, fn_signature_marker, end_marker="\n}\n"):
    b = block(src, fn_signature_marker, end_marker)
    return set(re.findall(r'"([a-zA-Z_][a-zA-Z0-9_]*)"', b))

def const_names(src, const_marker):
    """Names from a `const X: &[&str] = &[ ... ];` array.

    The gate used to read these out of the FUNCTION bodies. The lists were later refactored into
    consts, leaving `fn is_non_run_builtin` as a one-line `NON_RUN_BUILTINS.contains(&callee)` with
    no string literals in it at all — so the extraction silently returned an EMPTY set and the gate
    counted 196 where the real surface is 213, reporting 17 phantom `extra_in_sh`.

    A producer/consumer split of the same kind this repo has been closing all day: the names moved,
    the reader did not, and the failure looked like a drifted mirror rather than a blind gate.
    Reading both the function AND the const means neither refactor can hide the surface again.
    """
    i = src.find(const_marker)
    if i < 0:
        return set()
    end = src.index("];", i)
    return set(re.findall(r'"([a-zA-Z_][a-zA-Z0-9_]*)"', src[i:end]))

is_builtin_inline = names_in(run_src, "pub fn is_builtin_name(name: &str) -> bool {")
is_non_run = (names_in(run_src, "fn is_non_run_builtin(callee: &str) -> bool {")
              | const_names(run_src, "const NON_RUN_BUILTINS: &[&str] = &["))
is_poc_kit = (names_in(run_src, "fn is_poc_kit_builtin(callee: &str) -> bool {")
              | const_names(run_src, "const POC_KIT_BUILTINS: &[&str] = &["))
is_proof_input = (names_in(run_src, "fn is_proof_input_builtin(callee: &str) -> bool {")
                  | const_names(run_src, "const PROOF_INPUT_BUILTINS: &[&str] = &["))

run_all = emit_names | is_builtin_inline | is_non_run | is_poc_kit | is_proof_input

with open(sh_anb_path) as f:
    sh_src = f.read()
sh_block = block(sh_src, "fn sh_is_known_builtin(name) {", "\n}\n")
sh_names = set(re.findall(r'name == "([a-zA-Z_][a-zA-Z0-9_]*)"', sh_block))

missing_in_sh = sorted(run_all - sh_names)
extra_in_sh = sorted(sh_names - run_all)

print(f"CAPSET_REGISTRY_PARITY: run.rs={len(run_all)} sh_anb={len(sh_names)} "
      f"missing_in_sh={len(missing_in_sh)} extra_in_sh={len(extra_in_sh)}")

ok = True
if missing_in_sh:
    ok = False
    print(f"DRIFT: {len(missing_in_sh)} name(s) in run.rs are unrecognized by sh_is_known_builtin "
          f"(safe direction -- SH falls back to conservative/open -- but must be mirrored):")
    for n in missing_in_sh:
        print(f"  + {n}")
if extra_in_sh:
    ok = False
    print(f"BUG: {len(extra_in_sh)} name(s) in sh_is_known_builtin do not exist in run.rs at all "
          f"(the SH mirror is claiming to recognize something the host doesn't have):")
    for n in extra_in_sh:
        print(f"  - {n}")

if ok:
    print("CAPSET_REGISTRY_PARITY_GATE: PASS (sh_is_known_builtin is an exact mirror of is_builtin_name)")
    sys.exit(0)
else:
    print("CAPSET_REGISTRY_PARITY_GATE: FAIL -- mirror selfhost/src/anubis_sh.anb::sh_is_known_builtin "
          "against compiler/src/backends/run.rs::is_builtin_name")
    sys.exit(1)
PY
