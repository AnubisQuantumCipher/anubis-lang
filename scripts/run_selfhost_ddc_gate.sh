#!/usr/bin/env bash
# Phase A — Diverse Double-Compiling (DDC) gate. Fail-closed.
#
# Trusting-trust defense (Wheeler, https://dwheeler.com/trusting-trust).
#
# The self-host fixpoint (run_selfhost_gate.sh) proves the compiler reproduces
# itself byte-for-byte. But BOTH the Rust host and the emitted Anubis self-host
# lower to Rust and pass through the SAME rustc + LLVM. A Thompson-subverted
# rustc would sit in every one of those lanes and could hide a backdoor that
# survives the fixpoint (it would faithfully reproduce its own subversion).
#
# DDC closes that gap by introducing a SECOND, genuinely independent execution
# toolchain and requiring the two independently-built compilers to emit
# BYTE-IDENTICAL output for the same input:
#
#   cA = anubis_sh, executed by the REFERENCE interpreter compiled with rustc/LLVM
#        (selfhost/runtime/anubis_sh_interp_rt.rs -> rustc -> native binary).
#   cB = anubis_sh, executed by a faithful PORT of that interpreter compiled with
#        a NON-LLVM C compiler (selfhost/backend_c/anubis_sh_interp_rt.c -> gcc).
#
# Both run the identical anubis_sh compiler program (same AST payload). The ONLY
# variable is the native toolchain that produced the interpreter. If
# cA(anubis_sh.anb) and cB(anubis_sh.anb) emit the same bytes, a subversion
# hidden by rustc/LLVM in the compiler's machine code would have had to be
# independently reproduced, identically, by gcc — which is implausible.
#
# HONEST SCOPE (documented in docs/language/SELFHOST.md):
#   * DDC does NOT prove semantic correctness. It proves no SINGLE toolchain hid
#     a divergence in the compiler's executable behavior.
#   * The C compiler MUST NOT be clang: clang shares the LLVM backend with rustc,
#     so it would add no toolchain diversity. The gate refuses clang, fail-closed.
#   * The anubis_sh AST payload that BOTH engines run is itself derived through
#     the Rust host (there is no non-rustc Anubis parser yet). DDC here diversifies
#     the EXECUTION of that compiler program across two toolchains; it does not yet
#     diversify the source-level derivation of the payload AST. Writing a C-native
#     Anubis parser would close that residual. [NEEDS-HUMAN / future work]
#
# Load-bearing: a NEGATIVE CONTROL perturbs the C interpreter by one token and
# requires the gate to go red, proving the comparison is not trivially green.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/selfhost_ddc_gate}"
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi

rm -rf "$OUT"
mkdir -p "$OUT"

pass=0
fail=0
: >"$OUT/summary.txt"
note() { echo "  $1" | tee -a "$OUT/summary.txt"; }
pass_one() { pass=$((pass+1)); note "$1: PASS"; }
fail_one() { fail=$((fail+1)); note "$1: FAIL"; }

SELF="selfhost/src/anubis_sh.anb"
RT_RS="selfhost/runtime/anubis_sh_interp_rt.rs"
RT_C="selfhost/backend_c/anubis_sh_interp_rt.c"
BIN=./target/release/anubis

# --- Toolchain selection (fail-closed on clang) --------------------------------
# The second toolchain must be genuinely independent of rustc's LLVM backend.
pick_cc() {
  if [[ -n "${ANUBIS_DDC_CC:-}" ]]; then echo "$ANUBIS_DDC_CC"; return; fi
  for c in gcc-15 gcc-14 gcc-13 gcc-12 tcc; do
    if command -v "$c" >/dev/null 2>&1; then echo "$c"; return; fi
  done
  echo ""  # none found
}
CC="$(pick_cc)"
if [[ -z "$CC" ]]; then
  echo "SELFHOST_DDC_GATE: FAIL (no non-LLVM C compiler found; install gcc or tcc, or set ANUBIS_DDC_CC)"
  exit 1
fi
CC_VERSION_LINE="$("$CC" --version 2>/dev/null | head -1)"
# Refuse clang masquerading as gcc (Apple ships /usr/bin/gcc as clang).
if echo "$CC_VERSION_LINE" | grep -qi "clang"; then
  echo "SELFHOST_DDC_GATE: FAIL ('$CC' resolves to clang ($CC_VERSION_LINE); clang shares the LLVM backend with rustc and adds no toolchain diversity — set ANUBIS_DDC_CC to a real gcc/tcc)"
  exit 1
fi
RUSTC_VERSION_LINE="$(rustc --version 2>/dev/null)"
echo "== DDC toolchains =="
note "reference (cA): rustc  -> $RUSTC_VERSION_LINE"
note "diverse   (cB): $CC     -> $CC_VERSION_LINE"

cargo build -q --release -p anubis

# --- Payload: the anubis_sh compiler program (AST JSON) ------------------------
# Derived via the Rust host. BOTH engines run this identical program; the gate
# diversifies its EXECUTION, not its derivation (see HONEST SCOPE above).
echo "== derive anubis_sh AST payload =="
if "$BIN" run "$SELF" --allow-research --out "$OUT/host_parse" -- parse "$SELF" >"$OUT/payload.json" 2>"$OUT/payload.err" \
   && grep -q '"kind":"Program"' "$OUT/payload.json"; then
  PAYLOAD_SHA="$(shasum -a 256 "$OUT/payload.json" | awk '{print $1}')"
  note "payload sha256: $PAYLOAD_SHA"
  pass_one "ddc_payload_derived"
else
  fail_one "ddc_payload_derived"
  cat "$OUT/payload.err" >>"$OUT/summary.txt" 2>/dev/null || true
  echo "SELFHOST_DDC_GATE: FAIL (payload derivation failed)"; exit 1
fi

# --- Build cA: rustc-compiled reference interpreter + baked payload ------------
echo "== build cA (rustc/LLVM lane) =="
cA_ok=1
if ! "$BIN" run "$SELF" --allow-research --out "$OUT/host_emit" -- compile "$SELF" -o "$OUT/cA_src.rs" >"$OUT/cA_emit.log" 2>&1; then
  cA_ok=0
fi
# Sanity: emitted stage must be the interpreter package, not a payload-viewer.
if [[ $cA_ok -eq 1 ]] && { ! grep -q 'fn sh_run' "$OUT/cA_src.rs" || ! grep -q 'const PAYLOAD' "$OUT/cA_src.rs"; }; then
  cA_ok=0
fi
if [[ $cA_ok -eq 1 ]]; then
  if ! rustc -O -C codegen-units=1 -C debuginfo=0 "$OUT/cA_src.rs" -o "$OUT/cA" 2>"$OUT/cA_rustc.err"; then cA_ok=0; fi
fi
if [[ $cA_ok -eq 1 ]] && "$OUT/cA" version 2>/dev/null | grep -q "anubis-sh"; then
  pass_one "ddc_build_cA_rustc"
else
  fail_one "ddc_build_cA_rustc"
  cat "$OUT/cA_rustc.err" >>"$OUT/summary.txt" 2>/dev/null || true
  echo "SELFHOST_DDC_GATE: FAIL (cA build failed)"; exit 1
fi

# --- Build cB: gcc-compiled diverse interpreter (reads payload at runtime) -----
echo "== build cB ($CC / non-LLVM lane) =="
if "$CC" -O2 -std=c11 -Wall -Wextra -o "$OUT/cB" "$RT_C" 2>"$OUT/cB_cc.err" \
   && "$OUT/cB" "$OUT/payload.json" version 2>/dev/null | grep -q "anubis-sh"; then
  pass_one "ddc_build_cB_${CC//-/_}"
else
  fail_one "ddc_build_cB"
  cat "$OUT/cB_cc.err" >>"$OUT/summary.txt" 2>/dev/null || true
  echo "SELFHOST_DDC_GATE: FAIL (cB build failed)"; exit 1
fi

# --- DDC comparisons: cA vs cB must agree byte-for-byte ------------------------
# Compare COMPILER OUTPUT (not the two binaries — different toolchains legitimately
# yield different binaries; DDC is about output agreement).
ddc_cmp() { # <label> <command> <file>
  local label="$1" cmd="$2" file="$3"
  local a="$OUT/A_${label}.out" b="$OUT/B_${label}.out"
  set +e
  "$OUT/cA" "$cmd" "$file" >"$a" 2>"$OUT/A_${label}.err"; local ea=$?
  "$OUT/cB" "$OUT/payload.json" "$cmd" "$file" >"$b" 2>"$OUT/B_${label}.err"; local eb=$?
  set -e
  if [[ $ea -ne $eb ]]; then
    echo "exit mismatch $label: cA=$ea cB=$eb" >>"$OUT/ddc_fail.log"
    fail_one "ddc_${label}"
    return
  fi
  if cmp -s "$a" "$b"; then
    pass_one "ddc_${label}"
  else
    echo "byte mismatch $label" >>"$OUT/ddc_fail.log"
    cmp "$a" "$b" >>"$OUT/ddc_fail.log" 2>&1 || true
    fail_one "ddc_${label}"
  fi
}

echo "== DDC agreement: lex / parse / check over corpus =="
for f in selfhost/corpus/ok_*.anb; do
  b=$(basename "$f" .anb)
  ddc_cmp "lex_${b}" lex "$f"
  ddc_cmp "parse_${b}" parse "$f"
  ddc_cmp "check_${b}" check "$f"
done
# Failure paths must agree too (same diagnostic + same non-zero exit).
ddc_cmp "check_bad_type" check selfhost/corpus/bad_type.anb
ddc_cmp "check_bad_arity" check selfhost/corpus/bad_arity.anb
ddc_cmp "parse_bad" parse selfhost/corpus/bad_parse.anb

echo "== DDC CAPSTONE: cA vs cB emit the stage compiler from anubis_sh.anb =="
set +e
"$OUT/cA" compile "$SELF" -o "$OUT/stageA.rs" >"$OUT/capstone_A.log" 2>&1; ea=$?
"$OUT/cB" "$OUT/payload.json" compile "$SELF" -o "$OUT/stageB.rs" >"$OUT/capstone_B.log" 2>&1; eb=$?
set -e
if [[ $ea -eq 0 && $eb -eq 0 ]] && cmp -s "$OUT/stageA.rs" "$OUT/stageB.rs"; then
  OUTPUT_SHA="$(shasum -a 256 "$OUT/stageA.rs" | awk '{print $1}')"
  note "capstone: cA and cB emit BYTE-IDENTICAL stage source"
  note "agreed output sha256: $OUTPUT_SHA"
  pass_one "ddc_capstone_self_compile"
else
  echo "capstone divergence: cA_exit=$ea cB_exit=$eb" >>"$OUT/ddc_fail.log"
  cmp "$OUT/stageA.rs" "$OUT/stageB.rs" >>"$OUT/ddc_fail.log" 2>&1 || true
  OUTPUT_SHA="DIVERGED"
  fail_one "ddc_capstone_self_compile"
fi

# --- NEGATIVE CONTROL: the comparison must be load-bearing ---------------------
# Perturb the C interpreter by one token (append a stray byte on the general
# string-concat path) and require the capstone to DIVERGE. If it still matches,
# the gate is not actually comparing the two engines — fail closed.
echo "== negative control: perturb cB by one token, require divergence =="
NEG_C="$OUT/anubis_sh_interp_rt.perturbed.c"
cp "$RT_C" "$NEG_C"
python3 - "$NEG_C" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
old = ('            Str *out = str_new_cap(16);\n'
       '            display_into(out, l);\n'
       '            display_into(out, r);\n'
       '            return vs_take(out);')
new = ('            Str *out = str_new_cap(16);\n'
       '            display_into(out, l);\n'
       '            display_into(out, r);\n'
       '            str_push_bytes(out, "X", 1);\n'
       '            return vs_take(out);')
if old not in s:
    sys.stderr.write("negative-control anchor not found in C interpreter\n")
    sys.exit(3)
open(p, "w").write(s.replace(old, new, 1))
PY
neg_ok=0
if "$CC" -O2 -std=c11 -o "$OUT/cB_perturbed" "$NEG_C" 2>"$OUT/neg_cc.err"; then
  set +e
  "$OUT/cB_perturbed" "$OUT/payload.json" compile "$SELF" -o "$OUT/stageNeg.rs" >/dev/null 2>&1
  set -e
  if [[ -f "$OUT/stageNeg.rs" ]] && ! cmp -s "$OUT/stageA.rs" "$OUT/stageNeg.rs"; then
    neg_ok=1
  fi
fi
if [[ $neg_ok -eq 1 ]]; then
  note "negative control: one-token perturbation DIVERGES (gate is load-bearing)"
  pass_one "ddc_negative_control"
else
  note "negative control: perturbation did NOT diverge — gate is NOT load-bearing"
  fail_one "ddc_negative_control"
fi

# --- Manifest ------------------------------------------------------------------
python3 - "$OUT/ddc_manifest.json" "$RUSTC_VERSION_LINE" "$CC" "$CC_VERSION_LINE" \
  "$PAYLOAD_SHA" "$OUTPUT_SHA" "$pass" "$fail" <<'PY'
import json, sys
path, rustc_v, cc, cc_v, payload_sha, output_sha, npass, nfail = sys.argv[1:9]
m = {
  "gate": "selfhost_ddc",
  "claim": "Diverse Double-Compiling: two independent toolchains emit byte-identical compiler output",
  "reference_toolchain": {"role": "cA", "compiler": "rustc", "version": rustc_v,
                          "interpreter_source": "selfhost/runtime/anubis_sh_interp_rt.rs"},
  "diverse_toolchain":   {"role": "cB", "compiler": cc, "version": cc_v,
                          "interpreter_source": "selfhost/backend_c/anubis_sh_interp_rt.c"},
  "input": "selfhost/src/anubis_sh.anb",
  "payload_sha256": payload_sha,
  "agreed_output_sha256": output_sha,
  "result": ("PASS" if int(nfail) == 0 else "FAIL"),
  "checks_pass": int(npass),
  "checks_fail": int(nfail),
  "scope": "Diversifies EXECUTION of the compiler across two native toolchains. "
           "Does NOT prove semantic correctness, and does NOT yet diversify the "
           "source-level derivation of the payload AST (no C-native Anubis parser).",
}
open(path, "w").write(json.dumps(m, indent=2) + "\n")
PY
note "manifest: $OUT/ddc_manifest.json"

{
  echo "selfhost_ddc_gate pass=$pass fail=$fail"
  echo "cA=rustc/LLVM  cB=$CC/non-LLVM  input=$SELF  agreed_output_sha256=$OUTPUT_SHA"
} | tee -a "$OUT/summary.txt"

if [[ "$fail" -gt 0 ]]; then
  echo "SELFHOST_DDC_GATE: FAIL ($pass pass / $fail fail)"
  [[ -f "$OUT/ddc_fail.log" ]] && tail -20 "$OUT/ddc_fail.log"
  exit 1
fi
echo "SELFHOST_DDC_GATE: PASS ($pass/$pass)"
exit 0
