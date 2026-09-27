# Repeatable min/max keys — candidate withheld, 2026-09-26

**Not integrated.** The expanded required suite reports **16 passed, 3 failed**.
New baseline-valid precision regressions prohibit publication of this compiler
candidate. The original min/max findings remain open.

`withheld-candidate.patch` preserves the complete candidate against
`f48a5c9aa63125b9a069c4837bd42693a95a812c`, including its new semantic module,
tests, intended matrix outcomes and unintegrated CI roster. Do not apply its CI
roster as an approved passing lane. Source manifests and the immutable library
test instrument are retained separately; no new CLI pin was produced.

## What the candidate establishes and misses

It feeds earlier comparator-key labels into later incumbent arguments. A separate
bounded semantic summary recognizes fixed capture snapshots, exact literals,
ordered arrays, aliases/rebinding, lexical blocks, explicit returns and fully
evaluated statement-print arguments. It uses no abstract-value equality as runtime
key equality. All callable alternatives must qualify, and ordinary IFC still
checks callback effects and callable/collection control.

The initial required captured scalar/list key controls now pass full Safe checking.
The final focused suite initially reported **16 passed**; summary unit tests
reported **8 passed**. Independent review also found a continuation omission:
unknown-length iteration stopped when key labels stabilized even when an earlier
callback newly made continuation secret. `review-continuation-before.txt` records
the confirmed missing finding. The candidate now requires both selection-label
and continuation-PC stability; the new control passes for both operators.

Broader checking then found valid fixed keys the summary cannot yet recognize:
`s > 0`, `-s`, and `fixed(s)` with an unannotated helper returning its argument.
The old pin accepts all supplied min/max variants, but the candidate reports
secret egress. `precision-expansion/all-operators.txt` records both IFC2 and full
Safe refusals using a separately compiled, source-bound library harness.
`expanded-required-tests.txt` retains these as required **passing** tests and
records their failures. None was relabeled to make the candidate green.

This differs from the older-lane failures for printing the fixed winner and an
unused secret-printing singleton callback. The expanded baseline also refuses
those. The candidate restores IFC2 precision, but full checking still fails
through the ordinary lane. Their intended matrix outcome remains ACCEPT and
their precision debt remains OR-LANES-UNION.

## Evidence and next dependency

- `expanded-baseline-cases/` retains source hashes, commands, outputs and old-pin
  identity. The observed comparison reports 4 intended rejections accepted,
  16 intended acceptances accepted, and 4 intended acceptances refused.
- `integration-tests.txt` preserves the initial full-check precision failures;
  `integration-final.txt` and `summary-tests.txt` preserve the narrower passing
  stage. They are not the final compatibility verdict.
- `clippy.txt` passed before the expanded failing controls were added. The CLI
  build was deliberately stopped after confirming the regressions; its recorded
  exit is **143**, not a completed build or infrastructure failure.
- `candidate-test-pin.json` identifies the retained expanded library test
  executable. It is not an `anubis` CLI and is not bound to the bool-fix commit.
- `EXTENSION_DESIGN.md` and `DESIGN_REVIEW.md` specify the next dependency.

Next implement bounded stable unary/eager-comparison terms and resolved,
unannotated, nongeneric direct helpers with isolated frames, explicit returns,
exact arity and shared budgets. Do not enable arbitrary operators or helpers:
unmodeled traps can expose secret callback schedules. New code requires fresh
independent review and all required precision tests. No full-matrix, corpus,
workspace, guest, compatibility or mission completion is claimed here.
