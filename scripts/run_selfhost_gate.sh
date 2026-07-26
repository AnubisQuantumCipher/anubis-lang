#!/usr/bin/env bash
# Phase 8 — self-host gate (Anubis-SH). Fail-closed.
#
# Real bootstrap (not host×2 fake fixpoint):
#   stage0 (Rust host)  → stage1.rs → rustc → stage1
#   stage1              → stage2.rs → rustc → stage2
#   stage2              → stage3.rs
#   cmp stage2.rs stage3.rs
#
# Also: clean evidence dir, non-zero exits on parse/check fail,
# executable hello (not payload-viewer).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-out/selfhost_gate}"
if [[ "$OUT" != /* ]]; then OUT="$ROOT/$OUT"; fi

# Clean slate — never reuse stale evidence
rm -rf "$OUT"
mkdir -p "$OUT"

pass=0
fail=0
note() { echo "  $1" | tee -a "$OUT/summary.txt"; }
pass_one() { pass=$((pass+1)); note "$1: PASS"; }
fail_one() { fail=$((fail+1)); note "$1: FAIL"; }

: >"$OUT/summary.txt"
BIN=./target/release/anubis
cargo build -q --release -p anubis
# `anubis_sh.anb` (the self-hosted compiler's own source) contains zero
# `research{}`/`exploit{}` blocks and zero `@research`/`@exploit`/etc
# attributes (grep confirms this — the `target_run`/`p64`/`cyclic`/`shell`
# identifiers that DO appear in it are string-literal table entries its own
# sink/effect classifier uses to recognize those names in PROGRAMS IT
# COMPILES; anubis_sh.anb never calls them itself). Its inferred
# `program_mode` (tools/anubis/src/main.rs::program_mode, aggregating
# compiler/src/frontend/mod.rs::infer_mode per function) is therefore
# `Mode::Safe`, and `anubis run selfhost/src/anubis_sh.anb` needs no
# `--allow-research` at all — confirmed empirically: every subcommand below
# (version/lex/parse/check/compile, including the full self-compile that
# emits stage1.rs) succeeds on host with the flag OMITTED. `--allow-research`
# was dropped from SH_RUN for exactly this reason: it was never required by
# self-host semantics, and keeping it only meant this gate broke the moment
# commit 5fb7b67 (2026-07-25) made `--allow-research` VZ-guest-only
# (tools/anubis/src/offensive/isolation.rs::require_research_run_allowed).
# Removing the flag from a `Mode::Safe` program isn't a workaround — it's the
# gate correctly matching what `program_mode` already says about this source.
SH_RUN=("$BIN" run selfhost/src/anubis_sh.anb --out "$OUT/sh_run" --)

# Preflight: fail fast with the raw tool output if SH_RUN can't even print its
# own version, instead of letting every downstream check fail independently
# with no shared, up-front explanation.
preflight_out="$("${SH_RUN[@]}" version 2>&1)" || true
if ! grep -q "anubis-sh" <<<"$preflight_out"; then
  {
    echo "PREFLIGHT FATAL: \`\${SH_RUN[@]} version\` did not print the expected"
    echo "  'anubis-sh' banner. Every check below depends on SH_RUN working at"
    echo "  all, so they will likely fail identically. Raw tool output:"
    echo "  $preflight_out"
  } | tee -a "$OUT/summary.txt" >&2
  fail_one "preflight_sh_run_sanity"
fi

echo "== selfhost unit schema =="
if cargo test -p anubis-compiler --lib selfhost_schema -- --test-threads=4 >"$OUT/unit.log" 2>&1 \
  && grep -q "test result: ok" "$OUT/unit.log"; then
  pass_one "unit_selfhost_schema"
else
  fail_one "unit_selfhost_schema"
fi

echo "== args() / version =="
if "${SH_RUN[@]}" version >"$OUT/ver.log" 2>"$OUT/ver.err" && grep -q "anubis-sh" "$OUT/ver.log"; then
  pass_one "args_and_version"
else
  fail_one "args_and_version"
  cat "$OUT/ver.log" "$OUT/ver.err" >>"$OUT/summary.txt" || true
fi

echo "== host dump CLI =="
if "$BIN" selfhost dump-tokens selfhost/corpus/ok_hello.anb >"$OUT/host_tok.json" \
  && "$BIN" selfhost dump-ast selfhost/corpus/ok_hello.anb >"$OUT/host_ast.json" \
  && grep -q '"kind":"Keyword"' "$OUT/host_tok.json" \
  && grep -q '"kind":"Program"' "$OUT/host_ast.json"; then
  pass_one "host_dump_cli"
else
  fail_one "host_dump_cli"
fi

echo "== SELFHOST_A lex/parse goldens (stage0 host SH) =="
a_ok=1
for f in selfhost/corpus/ok_*.anb; do
  b=$(basename "$f" .anb)
  lex_ec=0; "${SH_RUN[@]}" lex "$f" >"$OUT/lex_${b}.json" 2>"$OUT/lex_${b}.err" || lex_ec=$?
  if [[ $lex_ec -ne 0 ]]; then
    a_ok=0
    echo "SH lex FAILED for $b: exit=$lex_ec, tool produced no/partial output — see $OUT/lex_${b}.err" >>"$OUT/a_fail.log"
    head -3 "$OUT/lex_${b}.err" >>"$OUT/a_fail.log" 2>/dev/null || true
  elif [[ ! -s "$OUT/lex_${b}.json" ]]; then
    a_ok=0
    echo "SH lex for $b produced no output (exit=0, empty stdout)" >>"$OUT/a_fail.log"
  elif [[ -f "selfhost/golden/tokens/${b}.json" ]]; then
    if ! diff -q "$OUT/lex_${b}.json" "selfhost/golden/tokens/${b}.json" >/dev/null; then
      if ! diff -q <(tr -d '\n' <"$OUT/lex_${b}.json") <(tr -d '\n' <"selfhost/golden/tokens/${b}.json") >/dev/null; then
        echo "token mismatch $b" >>"$OUT/a_fail.log"
        a_ok=0
      fi
    fi
  fi
  parse_ec=0; "${SH_RUN[@]}" parse "$f" >"$OUT/ast_${b}.json" 2>"$OUT/ast_${b}.err" || parse_ec=$?
  if [[ $parse_ec -ne 0 ]]; then
    a_ok=0
    echo "SH parse FAILED for $b: exit=$parse_ec, tool produced no/partial output — see $OUT/ast_${b}.err" >>"$OUT/a_fail.log"
    head -3 "$OUT/ast_${b}.err" >>"$OUT/a_fail.log" 2>/dev/null || true
  elif [[ ! -s "$OUT/ast_${b}.json" ]]; then
    a_ok=0
    echo "SH parse for $b produced no output (exit=0, empty stdout)" >>"$OUT/a_fail.log"
  elif [[ -f "selfhost/golden/ast/${b}.json" ]]; then
    python3 - "$OUT/ast_${b}.json" "selfhost/golden/ast/${b}.json" <<'PY' || { a_ok=0; echo "AST mismatch vs golden for $b" >>"$OUT/a_fail.log"; }
import json,sys
a=json.load(open(sys.argv[1])); b=json.load(open(sys.argv[2]))
def norm(x):
    if isinstance(x, dict):
        return {k:norm(v) for k,v in x.items() if not (k=="ty" and v in (None,""))}
    if isinstance(x, list):
        return [norm(i) for i in x]
    return x
if norm(a)!=norm(b):
    sys.exit(1)
PY
  fi
done
# bad parse must print error AND exit non-zero
set +e
"${SH_RUN[@]}" parse selfhost/corpus/bad_parse.anb >"$OUT/bad_parse.out" 2>&1
bp_ec=$?
set -e
if [[ $bp_ec -eq 0 ]] || ! grep -q PARSE_ERROR "$OUT/bad_parse.out"; then
  echo "bad_parse expected exit!=0 + PARSE_ERROR, got exit=$bp_ec" >>"$OUT/a_fail.log"
  a_ok=0
fi
if [[ $a_ok -eq 1 ]]; then pass_one "selfhost_a_lexparse"; else fail_one "selfhost_a_lexparse"; fi

echo "== SELFHOST_B checker (stage0) + fail exits =="
b_ok=1
if ! "${SH_RUN[@]}" check selfhost/corpus/ok_hello.anb 2>&1 | tee "$OUT/chk_ok.log" | grep -q "check passed"; then b_ok=0; fi
set +e
"${SH_RUN[@]}" check selfhost/corpus/bad_type.anb >"$OUT/chk_type.log" 2>&1
t_ec=$?
"${SH_RUN[@]}" check selfhost/corpus/bad_arity.anb >"$OUT/chk_arity.log" 2>&1
ar_ec=$?
set -e
if [[ $t_ec -eq 0 ]] || ! grep -q "ANUBIS_TYPE_MISMATCH" "$OUT/chk_type.log"; then b_ok=0; fi
if [[ $ar_ec -eq 0 ]] || ! grep -q "ANUBIS_SH_ARITY" "$OUT/chk_arity.log"; then b_ok=0; fi
if [[ $b_ok -eq 1 ]]; then pass_one "selfhost_b_check"; else fail_one "selfhost_b_check"; fi

echo "== SELFHOST_C hello: executable (not payload-viewer) =="
c_ok=1
c1_ec=0; "${SH_RUN[@]}" compile selfhost/corpus/ok_hello.anb -o "$OUT/v1_hello.rs" >"$OUT/c1.log" 2>&1 || c1_ec=$?
c2_ec=0; "${SH_RUN[@]}" compile selfhost/corpus/ok_hello.anb -o "$OUT/v2_hello.rs" >"$OUT/c2.log" 2>&1 || c2_ec=$?
if [[ $c1_ec -ne 0 || $c2_ec -ne 0 ]]; then
  c_ok=0
  echo "SH compile FAILED for ok_hello: exit1=$c1_ec exit2=$c2_ec, tool produced no/partial output — see $OUT/c1.log $OUT/c2.log" >>"$OUT/c_fail.log"
fi
if [[ $c_ok -eq 1 ]] && ! cmp -s "$OUT/v1_hello.rs" "$OUT/v2_hello.rs"; then
  echo "hello emit not deterministic" >>"$OUT/c_fail.log"
  c_ok=0
fi
if ! grep -q 'const PAYLOAD' "$OUT/v1_hello.rs" || ! grep -q 'fn sh_run' "$OUT/v1_hello.rs"; then
  echo "hello emit missing interpreter package" >>"$OUT/c_fail.log"
  c_ok=0
fi
# Reject pure payload-viewer (only prints payload_len)
if grep -q 'payload_len=' "$OUT/v1_hello.rs" && ! grep -q 'fn sh_run' "$OUT/v1_hello.rs"; then
  echo "payload-viewer emit rejected" >>"$OUT/c_fail.log"
  c_ok=0
fi
if ! rustc -O "$OUT/v1_hello.rs" -o "$OUT/hello_bin" 2>"$OUT/rustc_hello.err"; then
  c_ok=0
fi
if ! "$OUT/hello_bin" >"$OUT/hello_run.out" 2>&1; then c_ok=0; fi
if ! grep -q 'hello, anubis' "$OUT/hello_run.out"; then
  echo "hello binary did not print greeting: $(cat "$OUT/hello_run.out")" >>"$OUT/c_fail.log"
  c_ok=0
fi
if grep -q 'payload_len=' "$OUT/hello_run.out"; then
  echo "hello binary is still a payload-viewer" >>"$OUT/c_fail.log"
  c_ok=0
fi
if [[ $c_ok -eq 1 ]]; then pass_one "selfhost_c_hello_executable"; else fail_one "selfhost_c_hello_executable"; fi

echo "== self parse anubis_sh (stage0) =="
if "${SH_RUN[@]}" parse selfhost/src/anubis_sh.anb >"$OUT/self_ast.json" 2>"$OUT/self_parse.err" \
  && grep -q '"kind":"Program"' "$OUT/self_ast.json"; then
  pass_one "selfhost_parse_self"
else
  fail_one "selfhost_parse_self"
fi

# ---------------------------------------------------------------------------
# True bootstrap: stage0 → stage1 → stage2 → stage3; cmp stage2 stage3
# ---------------------------------------------------------------------------
echo "== BOOTSTRAP stage0 → stage1 =="
boot_ok=1
if ! "${SH_RUN[@]}" compile selfhost/src/anubis_sh.anb -o "$OUT/stage1.rs" >"$OUT/stage1_emit.log" 2>&1; then
  boot_ok=0
  note "stage0_emit_stage1: FAIL"
else
  note "stage0_emit_stage1: ok"
fi
if [[ $boot_ok -eq 1 ]]; then
  if ! grep -q 'fn sh_run' "$OUT/stage1.rs" || ! grep -q 'const PAYLOAD' "$OUT/stage1.rs"; then
    echo "stage1 missing interpreter/PAYLOAD" >>"$OUT/boot_fail.log"
    boot_ok=0
  fi
  if grep -q 'payload_len=' "$OUT/stage1.rs" && ! grep -q 'fn sh_run' "$OUT/stage1.rs"; then
    echo "stage1 is payload-viewer" >>"$OUT/boot_fail.log"
    boot_ok=0
  fi
fi
if [[ $boot_ok -eq 1 ]]; then
  if ! rustc -O "$OUT/stage1.rs" -o "$OUT/stage1" 2>"$OUT/rustc_stage1.err"; then
    boot_ok=0
    note "rustc_stage1: FAIL"
  else
    note "rustc_stage1: ok"
  fi
fi

# stage1 must be a working compiler
if [[ $boot_ok -eq 1 ]]; then
  if ! "$OUT/stage1" check selfhost/corpus/ok_hello.anb >"$OUT/s1_chk.log" 2>&1 \
    || ! grep -q "check passed" "$OUT/s1_chk.log"; then
    echo "stage1 check hello failed" >>"$OUT/boot_fail.log"
    boot_ok=0
  fi
  set +e
  "$OUT/stage1" check selfhost/corpus/bad_type.anb >"$OUT/s1_badtype.log" 2>&1
  s1t=$?
  set -e
  if [[ $s1t -eq 0 ]]; then
    echo "stage1 bad_type exited 0" >>"$OUT/boot_fail.log"
    boot_ok=0
  fi
  if ! "$OUT/stage1" compile selfhost/corpus/ok_hello.anb -o "$OUT/s1_hello.rs" >"$OUT/s1_hello_emit.log" 2>&1; then
    boot_ok=0
  fi
  if [[ $boot_ok -eq 1 ]]; then
    rustc -O "$OUT/s1_hello.rs" -o "$OUT/s1_hello" 2>"$OUT/s1_hello_rustc.err" || boot_ok=0
    if [[ $boot_ok -eq 1 ]]; then
      out_h=$("$OUT/s1_hello" 2>&1 || true)
      echo "$out_h" >"$OUT/s1_hello_run.out"
      if ! grep -q 'hello, anubis' <<<"$out_h"; then
        echo "stage1-produced hello did not run: $out_h" >>"$OUT/boot_fail.log"
        boot_ok=0
      fi
    fi
  fi
fi

echo "== BOOTSTRAP stage1 → stage2 =="
if [[ $boot_ok -eq 1 ]]; then
  if ! "$OUT/stage1" compile selfhost/src/anubis_sh.anb -o "$OUT/stage2.rs" >"$OUT/stage2_emit.log" 2>&1; then
    boot_ok=0
    note "stage1_emit_stage2: FAIL"
  else
    note "stage1_emit_stage2: ok"
  fi
fi
if [[ $boot_ok -eq 1 ]]; then
  if ! rustc -O "$OUT/stage2.rs" -o "$OUT/stage2" 2>"$OUT/rustc_stage2.err"; then
    boot_ok=0
    note "rustc_stage2: FAIL"
  else
    note "rustc_stage2: ok"
  fi
fi

echo "== BOOTSTRAP stage2 → stage3 + fixpoint =="
if [[ $boot_ok -eq 1 ]]; then
  if ! "$OUT/stage2" compile selfhost/src/anubis_sh.anb -o "$OUT/stage3.rs" >"$OUT/stage3_emit.log" 2>&1; then
    boot_ok=0
    note "stage2_emit_stage3: FAIL"
  else
    note "stage2_emit_stage3: ok"
  fi
fi
if [[ $boot_ok -eq 1 ]]; then
  if cmp -s "$OUT/stage2.rs" "$OUT/stage3.rs"; then
    note "cmp stage2.rs stage3.rs: identical"
    # Optional: stage1 should also match stage2 when host+SH are deterministic
    if cmp -s "$OUT/stage1.rs" "$OUT/stage2.rs"; then
      note "cmp stage1.rs stage2.rs: identical (host/SH agree)"
    else
      note "cmp stage1.rs stage2.rs: differ (allowed; fixpoint is stage2==stage3)"
    fi
    pass_one "selfhost_bootstrap_fixpoint"
  else
    echo "stage2/stage3 mismatch" >>"$OUT/boot_fail.log"
    wc -c "$OUT/stage2.rs" "$OUT/stage3.rs" >>"$OUT/boot_fail.log" || true
    fail_one "selfhost_bootstrap_fixpoint"
    boot_ok=0
  fi
else
  fail_one "selfhost_bootstrap_fixpoint"
  if [[ -f "$OUT/boot_fail.log" ]]; then
    tail -20 "$OUT/boot_fail.log" >>"$OUT/summary.txt" || true
  fi
fi

# ---------------------------------------------------------------------------
# Binary-level fixpoint (same-toolchain). The byte-identical fixpoint source,
# compiled through a pinned, reproducible rustc invocation (fixed codegen-units,
# no LC_UUID, path-remapped) with the Darwin ad-hoc code signature removed,
# yields BYTE-IDENTICAL native binaries. Upgrades the seal from source identity
# to binary identity.
#
# Claim scope (honest): same rustc + same flags only. LC_UUID and the ad-hoc
# code signature are content-derived Mach-O fields with no program semantics;
# they are normalized out. A DIFFERENT rustc version emits different code and is
# NOT expected to match — cross-toolchain-version binary identity is not claimed.
# ---------------------------------------------------------------------------
echo "== BINARY FIXPOINT stage2.bin == stage3.bin =="
if [[ -f "$OUT/stage2.rs" && -f "$OUT/stage3.rs" ]] && cmp -s "$OUT/stage2.rs" "$OUT/stage3.rs"; then
  # Compile stage2-content and stage3-content under an IDENTICAL canonical
  # filename (rustc embeds the source path in panic strings / module paths).
  # Keep LC_UUID so the binaries remain runnable on Apple Silicon (dyld refuses
  # to load a Mach-O with no LC_UUID).
  RFLAGS=(-O -C codegen-units=1 -C debuginfo=0 "--remap-path-prefix=$OUT=.")
  bin_ok=1
  cp "$OUT/stage2.rs" "$OUT/canon.rs"; rustc "${RFLAGS[@]}" "$OUT/canon.rs" -o "$OUT/stage2.bin" 2>"$OUT/rustc_bin2.err" || bin_ok=0
  cp "$OUT/stage3.rs" "$OUT/canon.rs"; rustc "${RFLAGS[@]}" "$OUT/canon.rs" -o "$OUT/stage3.bin" 2>"$OUT/rustc_bin3.err" || bin_ok=0
  if [[ $bin_ok -eq 1 ]]; then
    # Liveness: the compiled fixpoint binary is a real, runnable anubis-sh compiler.
    b2run=$("$OUT/stage2.bin" version 2>&1 || true)
    # Normalize COPIES for byte comparison: strip the ad-hoc code signature and
    # zero the content-derived LC_UUID (the only per-link nondeterministic fields).
    cp "$OUT/stage2.bin" "$OUT/stage2.norm"; cp "$OUT/stage3.bin" "$OUT/stage3.norm"
    if command -v codesign >/dev/null 2>&1; then
      codesign --remove-signature "$OUT/stage2.norm" "$OUT/stage3.norm" 2>/dev/null || true
    fi
    python3 "$ROOT/scripts/macho_normalize.py" "$OUT/stage2.norm" "$OUT/stage3.norm" >/dev/null 2>&1 || true
    if cmp -s "$OUT/stage2.norm" "$OUT/stage3.norm" && grep -q "anubis-sh" <<<"$b2run"; then
      bh=$(shasum -a 256 "$OUT/stage2.norm" 2>/dev/null | awk '{print $1}')
      echo "$bh" >"$OUT/binary_fixpoint.sha256"
      note "binary_fixpoint sha256 (LC_UUID + ad-hoc-sig normalized): $bh"
      pass_one "selfhost_binary_fixpoint"
    else
      echo "stage2.bin/stage3.bin differ after normalization (or not runnable)" >>"$OUT/boot_fail.log"
      fail_one "selfhost_binary_fixpoint"
    fi
  else
    echo "rustc of stageN.rs for binary fixpoint failed" >>"$OUT/boot_fail.log"
    cat "$OUT/rustc_bin2.err" "$OUT/rustc_bin3.err" >>"$OUT/summary.txt" 2>/dev/null || true
    fail_one "selfhost_binary_fixpoint"
  fi
else
  note "selfhost_binary_fixpoint: SKIP (source fixpoint not established)"
fi

{
  echo "selfhost_gate pass=$pass fail=$fail"
  echo "bootstrap: stage0(host) → stage1 → stage2 → stage3; seal=cmp(stage2,stage3) + binary fixpoint"
} | tee -a "$OUT/summary.txt"

if [[ "$fail" -gt 0 ]]; then
  echo "SELFHOST_GATE: FAIL ($pass pass / $fail fail)"
  exit 1
fi
echo "SELFHOST_GATE: PASS ($pass/$pass)"
exit 0
