#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

# Assert every security-critical walker reaches every expression-holding AST field.
#
# A `..` in a match arm has silently discarded a field FOUR times in `body_has_mode_elevator`, each
# shipping before it was found. The last let a mode elevator hide in a loop invariant — Safe-mode
# enforcement silently OFF, caught only because an unrelated check happened to fail.
#
# Totality over an enum stops a new VARIANT and does nothing about an arm that exists and ignores a
# field. This is the second half.
#
# --self-test reintroduces the real historical defect and proves the gate goes red, because a guard
# nobody has watched fail is a guard taken on faith.

WALKERS=(body_has_mode_elevator)

if [[ "${1:-}" == "--self-test" ]]; then
  BK="$(mktemp)"; cp compiler/src/middle/mod.rs "$BK"
  trap 'cp "$BK" compiler/src/middle/mod.rs' EXIT
  python3 - <<'PY'
p='compiler/src/middle/mod.rs'; s=open(p).read()
old = """            Stmt::While {
                cond,
                body,
                invariant,
            } => in_expr(cond) || invariant.iter().any(in_expr) || in_stmts(body),"""
new = """            Stmt::While { cond, body, .. } => in_expr(cond) || in_stmts(body),"""
if old in s:
    open(p,'w').write(s.replace(old,new,1))
    print("self-test: reintroduced While-drops-invariant")
else:
    raise SystemExit("self-test: could not plant the defect (walker shape changed)")
PY
  set +e
  python3 scripts/lib/walker_completeness.py "${WALKERS[@]}"
  rc=$?
  set -e
  if [[ $rc -eq 0 ]]; then
    echo "SELFTEST FAIL: gate stayed green with the defect planted"
    exit 1
  fi
  echo "SELFTEST PASS: gate went red on the planted defect (rc=$rc)"
  exit 0
fi

python3 scripts/lib/walker_completeness.py "${WALKERS[@]}"
rc=$?
if [[ $rc -eq 0 ]]; then echo "WALKER_COMPLETENESS_GATE: PASS"; else echo "WALKER_COMPLETENESS_GATE: FAIL"; fi
exit $rc
