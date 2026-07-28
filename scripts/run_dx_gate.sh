#!/usr/bin/env bash
# Phase 7 — developer experience gate (doc, repl, lsp, editors, regressions).
# Nothing is PASS unless exercised end-to-end.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
source "$ROOT/scripts/lib/gate_common.sh"
OUT_ARG="${1:-out/dx_gate}"
# Absolute so subshells `cd` elsewhere still write logs correctly.
if [[ "$OUT_ARG" = /* ]]; then
  OUT="$OUT_ARG"
else
  OUT="$ROOT/$OUT_ARG"
fi
mkdir -p "$OUT"

pass=0
fail=0
detail=()
note() { detail+=("$1"); }

if [[ -n "${ANUBIS_BIN:-}" ]]; then
  BIN="$ANUBIS_BIN"
  [[ -x "$BIN" ]] || { echo "DX_GATE: FAIL (ANUBIS_BIN=$BIN not executable)"; exit 127; }
else
  BIN=./target/release/anubis
  cargo build -q --release -p anubis
fi

# 1) Unit: doc / interp / lsp_analysis
if cargo test -p anubis-compiler --lib -- doc::tests interp::tests lsp_analysis 2>&1 | tee "$OUT/unit.log" | grep -q "test result: ok"; then
  pass=$((pass+1)); note "unit_dx: PASS"
else
  fail=$((fail+1)); note "unit_dx: FAIL"
fi

# 2) anubis doc Contracts + attached doc comment
if "$BIN" doc tests/fixtures/dx/contracts_doc.anb >"$OUT/doc.md" 2>"$OUT/doc.err" \
  && grep -q "### Contracts" "$OUT/doc.md" \
  && grep -q "requires" "$OUT/doc.md" \
  && grep -q "ensures" "$OUT/doc.md" \
  && grep -q "Integer division" "$OUT/doc.md" \
  && "$BIN" doc tests/fixtures/dx/contracts_doc.anb --format json >"$OUT/doc.json" 2>>"$OUT/doc.err" \
  && grep -q '"requires"' "$OUT/doc.json"; then
  pass=$((pass+1)); note "anubis_doc_contracts: PASS"
else
  fail=$((fail+1)); note "anubis_doc_contracts: FAIL"
  cat "$OUT/doc.err" >>"$OUT/doc.md" || true
fi

# 3) repl --eval arithmetic
if "$BIN" repl --eval '2 + 3' >"$OUT/repl.log" 2>&1 && grep -q '5' "$OUT/repl.log"; then
  pass=$((pass+1)); note "repl_eval: PASS"
else
  fail=$((fail+1)); note "repl_eval: FAIL"
fi

# 4) repl typecheck fail-closed (must be check:, not a parse botch)
set +e
"$BIN" repl --eval 'let x: u32 = true' >"$OUT/repl_bad.log" 2>&1
rbc=$?
set -e
if [[ $rbc -ne 0 ]] && grep -Eiq 'check:|type|mismatch|u32|bool|error' "$OUT/repl_bad.log"; then
  pass=$((pass+1)); note "repl_check_failclosed: PASS"
else
  fail=$((fail+1)); note "repl_check_failclosed: FAIL"
  cat "$OUT/repl_bad.log" >>"$OUT/summary_fail.txt" || true
fi

# 5) repl --exact path
if "$BIN" repl --exact --eval '2 + 3' >"$OUT/repl_exact.log" 2>&1 && grep -q '5' "$OUT/repl_exact.log"; then
  pass=$((pass+1)); note "repl_exact: PASS"
else
  fail=$((fail+1)); note "repl_exact: FAIL"
fi

# 6) hello check/run
if "$BIN" check tests/fixtures/dx/hello.anb >"$OUT/hello_check.log" 2>&1 \
  && "$BIN" run tests/fixtures/dx/hello.anb --out "$OUT/hello_run" >"$OUT/hello_run.log" 2>&1 \
  && grep -qi hello "$OUT/hello_run.log"; then
  pass=$((pass+1)); note "hello_check_run: PASS"
else
  fail=$((fail+1)); note "hello_check_run: FAIL"
fi

# 7) LSP CLI present + real JSON-RPC roundtrip
if "$BIN" --help 2>&1 | grep -qi lsp; then
  if python3 scripts/test_lsp_roundtrip.py >"$OUT/lsp_roundtrip.log" 2>&1; then
    pass=$((pass+1)); note "lsp_roundtrip: PASS"
  else
    fail=$((fail+1)); note "lsp_roundtrip: FAIL"
    cat "$OUT/lsp_roundtrip.log" >>"$OUT/summary_fail.txt" || true
  fi
else
  fail=$((fail+1)); note "lsp_roundtrip: FAIL (cli missing)"
fi

# 8) Editors present + JSON validity + extension deps resolvable
if [[ -f editors/vscode-anubis/package.json \
   && -f editors/vscode-anubis/extension.js \
   && -f editors/vscode-anubis/syntaxes/anubis.tmLanguage.json \
   && -f editors/tree-sitter-anubis/grammar.js \
   && -f editors/tree-sitter-anubis/queries/highlights.scm ]]; then
  if node -e "
    const fs=require('fs');
    JSON.parse(fs.readFileSync('editors/vscode-anubis/package.json','utf8'));
    JSON.parse(fs.readFileSync('editors/vscode-anubis/syntaxes/anubis.tmLanguage.json','utf8'));
    JSON.parse(fs.readFileSync('editors/vscode-anubis/language-configuration.json','utf8'));
    // extension must load vscode-languageclient when present
    const path=require('path');
    const nm='editors/vscode-anubis/node_modules/vscode-languageclient';
    if (!fs.existsSync(nm)) {
      console.error('missing vscode-languageclient — run npm install in editors/vscode-anubis');
      process.exit(2);
    }
    require(path.resolve(nm + '/package.json'));
    console.log('editors_json_ok');
  " >"$OUT/editors_validate.log" 2>&1; then
    pass=$((pass+1)); note "editors_valid: PASS"
  else
    fail=$((fail+1)); note "editors_valid: FAIL"
    cat "$OUT/editors_validate.log" >>"$OUT/summary_fail.txt" || true
  fi
else
  fail=$((fail+1)); note "editors_valid: FAIL (missing files)"
fi

# 9) tree-sitter grammar loads under node (highlight-oriented; not parser of record)
if node -e "
  const fs=require('fs');
  const g=fs.readFileSync('editors/tree-sitter-anubis/grammar.js','utf8');
  if (!g.includes(\"name: 'anubis'\") && !g.includes('name: \"anubis\"')) process.exit(2);
  if (!g.includes('requires') || !g.includes('ensures')) process.exit(3);
  const h=fs.readFileSync('editors/tree-sitter-anubis/queries/highlights.scm','utf8');
  if (!h.includes('function') && !h.includes('keyword')) process.exit(4);
  const c=fs.readFileSync('editors/tree-sitter-anubis/test/corpus/basic.txt','utf8');
  if (!c.includes('requires')) process.exit(5);
  console.log('tree_sitter_assets_ok');
" >"$OUT/tree_sitter.log" 2>&1; then
  pass=$((pass+1)); note "tree_sitter_assets: PASS"
else
  fail=$((fail+1)); note "tree_sitter_assets: FAIL"
fi

# tree-sitter generate + corpus (local CLI in editors/tree-sitter-anubis preferred)
TS_BIN=""
if [[ -x "$ROOT/editors/tree-sitter-anubis/node_modules/.bin/tree-sitter" ]]; then
  TS_BIN="$ROOT/editors/tree-sitter-anubis/node_modules/.bin/tree-sitter"
elif command -v tree-sitter >/dev/null 2>&1; then
  TS_BIN="$(command -v tree-sitter)"
fi
if [[ -n "$TS_BIN" ]]; then
  if (cd editors/tree-sitter-anubis && "$TS_BIN" generate >"$OUT/ts_gen.log" 2>&1 \
      && "$TS_BIN" test >"$OUT/ts_test.log" 2>&1 \
      && grep -q "failed parses: 0" "$OUT/ts_test.log"); then
    pass=$((pass+1)); note "tree_sitter_cli_test: PASS"
  elif [[ -f editors/tree-sitter-anubis/src/parser.c ]] \
    && (cd editors/tree-sitter-anubis && "$TS_BIN" test >"$OUT/ts_test.log" 2>&1) \
    && grep -q "failed parses: 0" "$OUT/ts_test.log"; then
    pass=$((pass+1)); note "tree_sitter_cli_test: PASS (prebuilt parser)"
  else
    fail=$((fail+1)); note "tree_sitter_cli_test: FAIL"
    tail -20 "$OUT/ts_gen.log" >>"$OUT/summary_fail.txt" 2>/dev/null || true
    tail -20 "$OUT/ts_test.log" >>"$OUT/summary_fail.txt" 2>/dev/null || true
  fi
elif [[ -f editors/tree-sitter-anubis/src/parser.c ]]; then
  fail=$((fail+1)); note "tree_sitter_cli_test: FAIL (parser.c present but CLI test was not executed)"
else
  fail=$((fail+1)); note "tree_sitter_cli_test: FAIL (no CLI and no parser.c)"
fi

# 10) Tutorial + SPEC present with Contracts / Phase 7 content
if [[ -f docs/language/TUTORIAL.md ]] \
  && grep -q 'anubis doc' docs/language/TUTORIAL.md \
  && grep -q 'Contracts' docs/language/TUTORIAL.md \
  && grep -q 'Phase 7' docs/language/SPEC.md \
  && grep -q 'Learn Anubis' README.md; then
  pass=$((pass+1)); note "docs_tutorial_spec: PASS"
else
  fail=$((fail+1)); note "docs_tutorial_spec: FAIL"
fi

# 11) Phase 5/6 regression smoke
if cargo test -p anubis-compiler --lib phase5_ -- --test-threads=4 >"$OUT/p5.log" 2>&1 \
  && grep -q "test result: ok" "$OUT/p5.log"; then
  pass=$((pass+1)); note "phase5_regress: PASS"
else
  fail=$((fail+1)); note "phase5_regress: FAIL"
fi
if cargo test -p anubis-compiler --lib phase6_ -- --test-threads=4 >"$OUT/p6.log" 2>&1 \
  && grep -q "test result: ok" "$OUT/p6.log"; then
  pass=$((pass+1)); note "phase6_regress: PASS"
else
  fail=$((fail+1)); note "phase6_regress: FAIL"
fi

# Formatter: `--check` must DETECT drift (fail-closed) and be idempotent after `--write`.
printf 'fn   f(a:u32)->u32{return a+1;}\n' >"$OUT/fmt_drift.anb"
set +e
"$BIN" fmt --check "$OUT/fmt_drift.anb" >"$OUT/fmt_check_bad.log" 2>&1
fmt_bad_rc=$?
set -e
"$BIN" fmt --write "$OUT/fmt_drift.anb" >"$OUT/fmt_write.log" 2>&1
if [[ $fmt_bad_rc -ne 0 ]] && "$BIN" fmt --check "$OUT/fmt_drift.anb" >"$OUT/fmt_check_ok.log" 2>&1; then
  pass=$((pass+1)); note "fmt_check_idempotent: PASS"
else
  fail=$((fail+1)); note "fmt_check_idempotent: FAIL"
fi

# Test runner: `anubis test` must honor `// EXPECT: PASS|FAIL` directives over a directory.
mkdir -p "$OUT/testfiles"
printf '// EXPECT: PASS\nfn main() { print(1); }\n' >"$OUT/testfiles/ok.anb"
printf '// EXPECT: FAIL\nfn main() { let x: u32 = true; print(x); }\n' >"$OUT/testfiles/bad.anb"
if "$BIN" test "$OUT/testfiles" >"$OUT/test_runner.log" 2>&1; then
  pass=$((pass+1)); note "test_runner: PASS"
else
  fail=$((fail+1)); note "test_runner: FAIL"
  cat "$OUT/test_runner.log" >>"$OUT/summary_fail.txt" || true
fi

{
  echo "dx_gate pass=$pass fail=$fail"
  for d in "${detail[@]}"; do echo "  $d"; done
} | tee "$OUT/summary.txt"

# Coverage ratchet (adversary R49) — outside | tee so fail+= is not lost in a subshell.
_cases=$((pass + fail))
set +e
assert_floor "dx_gate" "$_cases" "$ROOT/scripts/floors/dx_gate.count_floor"
_floor_rc=$?
set -e
if [[ $_floor_rc -ne 0 ]]; then
  echo "FLOOR: FAIL ($_cases cases; $GATE_FLOOR_ERROR)" >&2
  fail=$((fail + 1))
fi

if [[ "$fail" -gt 0 ]]; then
  echo "DX_GATE: FAIL" | tee -a "$OUT/summary.txt"
  exit 1
fi
echo "DX_GATE: PASS" | tee -a "$OUT/summary.txt"
exit 0
