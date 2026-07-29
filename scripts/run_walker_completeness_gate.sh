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

# Each entry is `name` or `name:scope` (scope = all|expr|stmt).
#
# COVERAGE CAN ONLY INCREASE. This registry had exactly ONE walker for its whole life, while the
# adversary censused TWENTY-SIX independent value-flow walkers whose count is RISING because every
# parity fix spawns a twin (`scratchpad/fleet_20260726/adversary_round21.md`). The mechanism was
# never the problem; adoption was — the same story as `scripts/lib/gate_common.sh`.
#
# The reason adoption was stuck: before the `scope` argument existed this check demanded every
# walker bind every code-holding field of BOTH `Stmt` and `Expr`, so an expression-only query
# scored eleven `Stmt::* is never matched` non-defects and drowned the one real finding. A gate
# whose output is mostly false positives gets one walker registered and then abandoned.
#
# Adding a walker here is a claim that it must be TOTAL over its scope. Do not add one to make a
# number look better — add it when it passes, and fix it first when it does not.
WALKERS=(
  body_has_mode_elevator
  analyze_expr_effect:expr
  # Registered 2026-07-28 once they reached zero problems. All three had discarded `Expr::If.cond`
  # through a `..` while `Expr::IfLet` thirty lines above bound and consulted its scrutinee — one
  # shape, three functions, and the effect lane clean. That gap was a real leak: a `let` whose init
  # was a secret-SELECTED constant passed `check` and printed the value. Registering them means a
  # future `..` in this position breaks the GATE rather than reopening the leak.
  expr_taint_source_m:expr
  expr_secret_source_m:expr
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
  # Registered 2026-07-28 once the checker gained tuple-variant parsing AND a `pattern` scope.
  #
  # These four were documented here as passing VACUOUSLY — "no inspectable arms". That was wrong
  # about the cause. `propagate_pattern_closures` binds TEN `Pattern::` variants and
  # `seed_secret_pattern` eight `Stmt::` ones; they looked empty because the checker only ever
  # examined `Expr`, and because `pattern` was implemented in the variant mapping but missing from
  # the SCOPES allow-list, so every Pattern-scoped registration was rejected as an unknown scope.
  #
  # A walker that "passes vacuously" and a walker the CHECKER cannot see are indistinguishable from
  # the outside, and the note claiming the former stood for a day. Registry 9 -> 13.
  propagate_pattern_closures:partial-pattern
  seed_taint_pattern:partial-pattern
  seed_effect_pattern:partial-expr
  seed_secret_pattern:partial-stmt
)

# NOT REGISTERED, and the reason is the point.
#
# `seed_taint_pattern`, `seed_effect_pattern`, `seed_secret_pattern` and
# `propagate_pattern_closures` all PASS `partial-expr` — and all four pass it VACUOUSLY.
#
# The first three match zero `Expr`, `Stmt` and `Pattern` variants by name, so there is no arm to
# inspect and "OK" means only "nothing to look at". The fourth is subtler and nearly fooled me: its
# sole match is `Expr::Var(sv)`, a TUPLE variant, while `enum_variants` parses only brace-struct
# variants — so the checker tracks no fields for it and again has nothing to bind.
#
# A registry entry is a CLAIM that a walker is constrained. An entry a walker satisfies by having no
# inspectable arms is the same defect as a gate that passes an empty corpus, which is the class this
# harness exists to catch. Registering these four would take the count from 10 to 14 and constrain
# nothing.
#
# Both gaps are real and named: a Pattern-aware contract, and tuple-variant field tracking. Neither
# exists yet, so these four stay OUT.

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
  trap 'rm -f "$SCRATCH"' EXIT
  cp compiler/src/middle/mod.rs "$SCRATCH"
  export ANUBIS_WALKER_MID="$SCRATCH"
  python3 - "$SCRATCH" <<'PY'
import sys
p=sys.argv[1]; s=open(p).read()
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
