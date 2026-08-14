#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"

# Assert every security-critical walker DIRECTLY uses or explicitly DEFERS every code-holding field.
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

# Each entry is `name` or `name:scope`; accepted scope names include Expr, Stmt, Pattern, and
# explicit `partial-` specialization. Pattern remains fail-closed until its recursive fields and
# scalar match identity have a truthful classifier; both full and partial zero-arm registrations
# are rejected as vacuous.
#
# EFFECTIVE COVERAGE CAN ONLY INCREASE. A registration that matches zero tracked code-bearing arms
# is not coverage and must be removed or repaired, never counted. This registry had exactly ONE
# walker for its whole life, while the
# adversary censused TWENTY-SIX independent value-flow walkers whose count is RISING because every
# parity fix spawns a twin (`scratchpad/fleet_20260726/adversary_round21.md`). The mechanism was
# never the problem; adoption was — the same story as `scripts/lib/gate_common.sh`.
#
# The reason adoption was stuck: before the `scope` argument existed this check demanded every
# walker dispose every code-holding field of BOTH `Stmt` and `Expr`, so an expression-only query
# scored eleven `Stmt::* is never matched` non-defects and drowned the one real finding. A gate
# whose output is mostly false positives gets one walker registered and then abandoned.
#
# Adding a walker here is a claim that it must dispose every code-holding field in its scope. The formerly duplicated
# taint/secret block walkers were registered while RED during Phase 0; their shared replacement is
# now the one load-bearing statement traversal for both domains.
WALKERS=(
  body_has_mode_elevator
  analyze_expr_effect:expr
  # Shared integrity/confidentiality value-block traversal. Domain-specific wrappers provide label
  # operations but do not own AST descent, so walker parity is enforced structurally rather than by
  # keeping two match trees synchronized by hand.
  walk_block_labels:stmt
  # Registered 2026-07-28 once they reached zero problems. All three had discarded `Expr::If.cond`
  # through a `..` while `Expr::IfLet` thirty lines above bound and consulted its scrutinee — one
  # shape, three functions, and the effect lane clean. That gap was a real leak: a `let` whose init
  # was a secret-SELECTED constant passed `check` and printed the value. Registering them means a
  # future `..` in this position breaks the GATE rather than reopening the leak.
  expr_source:expr
  expr_param_flow:expr
  # `partial-stmt` = a SPECIALISED walker's contract: it need not match every variant, but every
  # variant it DOES match must bind all that variant's code-holding fields.
  #
  # These two extract a block's value from its last statement. They were written to CLOSE the
  # `Expr::If` cond-drop and they REPRODUCED it — `Stmt::If { then, else_, .. }`, cond discarded,
  # so a secret condition selecting between two clean constants stayed invisible. Same `..`, third
  # place, inside its own repair. A total-coverage demand could not express their contract (they
  # deliberately read only the last statement, scoring ten "never matched" non-defects and burying
  # the one real finding), which is exactly why this contract exists.
  stmt_value_secret:partial-stmt
  stmt_value_taint:partial-stmt
  # The three sibling modules that own their own `walk_expr`. They were unregisterable for a
  # trivial reason — the checker only ever read middle/mod.rs — not because of anything about the
  # walkers. Names are QUALIFIED because all three define `walk_expr`; an unqualified lookup would
  # silently grade whichever file was searched first, which is a gate quietly measuring a different
  # walker than the registry names. Unqualified now refuses and lists the candidates.
  effects::walk_expr:partial-expr
  capability::walk_expr:partial-expr
  trifecta::walk_expr:partial-expr
)

# DELIBERATELY NOT REGISTERED:
#   propagate_pattern_closures
#   seed_taint_pattern
#   seed_effect_pattern
#   seed_secret_pattern
#
# The non-vacuity floor proves each proposed current scope matches zero code-bearing arms and emits
# `WALKER_PARTIAL_VACUOUS`. A registry entry is a claim that a walker is constrained; retaining an
# entry that inspects no tracked arm would be equivalent to passing an empty corpus. These functions
# therefore remain an explicit open coverage class until the checker has a truthful contract for
# their Pattern/seeding semantics. They must not be described as protected by this gate.

bash scripts/test_walker_completeness.sh

if [[ "${1:-}" == "--self-test" ]]; then
  # Plant the defect in a SCRATCH COPY, never in the live file.
  #
  # This self-test used to `cp` mod.rs aside, mutate it in place, and restore it from a trap. That
  # is a silent data-loss window for any concurrent writer: an agent saving mod.rs between the
  # backup and the restore has its work overwritten with no error and no diff. This repo has
  # already lost in-progress work to exactly that shape once — docs/COMMIT_5259227_CORRECTION.md —
  # and a gate's own self-test is the last place that should be able to cause it a second time.
  SCRATCH="$(mktemp -t anubis_walker_mid).rs"
  BASELINE="$(mktemp -t anubis_walker_baseline).txt"
  PLANTED="$(mktemp -t anubis_walker_planted).txt"
  trap 'rm -f "$SCRATCH" "$BASELINE" "$PLANTED"' EXIT

  # A poison is calibrated only against a green baseline. Require this exact finding to be absent
  # before the plant, present after it, and the sole structured failure in the planted run.
  set +e
  python3 scripts/lib/walker_completeness.py --require-all-deferred "${WALKERS[@]}" >"$BASELINE" 2>&1
  baseline_rc=$?
  set -e

  cp compiler/src/middle/mod.rs "$SCRATCH"
  python3 - "$SCRATCH" <<'PY'
import sys
p=sys.argv[1]; s=open(p).read()
old = """            Stmt::While {
                cond,
                body,
                invariant,
            } => in_expr(cond) || invariant.iter().any(in_expr) || in_stmts(body),"""
new = """            Stmt::While { cond, body, .. } => in_expr(cond) || in_stmts(body),"""
if s.count(old) != 1:
    raise SystemExit(f"self-test: defect anchor count must be 1, got {s.count(old)}")
open(p,'w').write(s.replace(old,new,1))
print("self-test: reintroduced While-drops-invariant")
PY
  set +e
  python3 - "$SCRATCH" --require-all-deferred "${WALKERS[@]}" >"$PLANTED" 2>&1 <<'PY'
import sys
from pathlib import Path

import scripts.lib.walker_completeness as walker

walker.MID = Path(sys.argv[1])
sys.argv = ["walker_completeness.py", *sys.argv[2:]]
raise SystemExit(walker.main())
PY
  rc=$?
  set -e
  cat "$PLANTED"
  needle='WALKER_UNBOUND_FIELD walker=body_has_mode_elevator variant=Stmt::While field=invariant arms=1'
  planted_problem_count="$(grep -Fc 'WALKER_UNBOUND_FIELD ' "$PLANTED" || true)"
  if [[ $baseline_rc -ne 0 ]]; then
    echo "SELFTEST FAIL: baseline gate was not green (baseline rc=$baseline_rc)"
    exit 1
  fi
  if grep -Fq "$needle" "$BASELINE"; then
    echo "SELFTEST FAIL: planted finding was already present at baseline (baseline rc=$baseline_rc)"
    exit 1
  fi
  if ! grep -Fq "$needle" "$PLANTED"; then
    echo "SELFTEST FAIL: planted finding was absent after mutation (planted rc=$rc)"
    exit 1
  fi
  if [[ $rc -ne 1 ]]; then
    echo "SELFTEST FAIL: planted checker rc was $rc, expected exactly 1"
    exit 1
  fi
  if [[ $planted_problem_count -ne 1 ]]; then
    echo "SELFTEST FAIL: planted run emitted $planted_problem_count structured findings, expected 1"
    exit 1
  fi
  if ! grep -Fxq 'WALKER_COMPLETENESS: FAIL (1)' "$PLANTED"; then
    echo "SELFTEST FAIL: planted summary was not exactly WALKER_COMPLETENESS: FAIL (1)"
    exit 1
  fi
  echo "SELFTEST PASS: exact planted finding absent at baseline (rc=$baseline_rc), present after mutation (rc=$rc)"
  exit 0
fi

set +e
python3 scripts/lib/walker_completeness.py --require-all-deferred "${WALKERS[@]}"
rc=$?
set -e
if [[ $rc -eq 0 ]]; then echo "WALKER_COMPLETENESS_GATE: PASS"; else echo "WALKER_COMPLETENESS_GATE: FAIL"; fi
exit $rc
