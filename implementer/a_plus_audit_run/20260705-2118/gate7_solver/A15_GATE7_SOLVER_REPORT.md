# A15 Gate 7 Solver Report

**Stamp:** 20260705-2118
**Reproduced by fresh commands.**

## Verdicts
* Gate 7 faithful solver semantics: YES (BV, defs for derived, inlined exprs)
* Prior unconstrained-result bug fixed: YES (defs bind result = expr; no free result=30)
* Counterexample replay: YES (basic validator returns valid for models)
* Bitvector/width/overflow semantics explicit: PARTIAL (32 default, u8 example runs, semantics in smt QF_BV)
* Solver SARIF quality: PARTIAL (rules from earlier, solver checks in SARIF)
* Solver evidence integration: YES (solver.smt2, solver_replay.json in analysis/, verify passes)
* Solver tamper detection: YES (hash mismatch detected)

## Commands Executed
(See GATING_EVIDENCE.log for full output with safety, checks, verifies, tamper, tests.)

Bundles and analysis files copied.

**One-sentence:** Solver now uses faithful BV expressions with derived bindings; pass proves, fail gives model; replay and evidence integrated; old bug class prevented.

