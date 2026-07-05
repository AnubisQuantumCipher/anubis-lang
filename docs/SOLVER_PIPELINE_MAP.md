# Anubis Solver Pipeline Map (Gate 7)

**Date:** 2026-07-05
**Purpose:** Document current solver path for faithful SMT implementation in Gate 7.

## 1. Source Parser Entry
- File: `compiler/src/frontend/mod.rs`
- Functions: `parse_source`, `parse_source_detailed`, `parse_assume_or_assert`, handling for `symbolic`, `taint_source`, `assume`, `assert`.
- AST nodes:
  - `Expr::Symbolic { ty: String }`
  - `Expr::Assume(Box<Expr>)`
  - `Expr::Assert(Box<Expr>)`
  - `Expr::TaintSource { label: String }`
  - `Stmt::ExprStmt(Expr::Assume(...))` or `Assert(...)`

## 2. AST Representation
- Symbolic variables: `symbolic::<T>()` or `symbolic("name")` (currently type-focused or label).
- `assume(expr)` and `assert(expr)` parsed as ExprStmt.
- Expressions: Binary ops, literals, vars, declassify, taint_source.

## 3. HIR/MIR Representation
- File: `compiler/src/middle/mod.rs`
- `TypedIR` has:
  - `constraints: Vec<String>` (SMT-like strings)
  - `solver_obligations: Vec<SolverObligation>`
  - `body: Vec<Stmt>`
- `analyze_stmts` collects:
  - For Assume: `assumptions.push(smt); ctx.constraints.push(...)`
  - For Assert: creates `SolverObligation { name, assumptions, assertion: smt, vars }`; pushes to constraints.
- HIR: `HirFunction`, symbols with taint.
- MIR: `MirBlock` (light).

## 4. Solver Lowering Files/Functions
- `middle/mod.rs`:
  - `SymbolicEngine::generate_constraints(source) -> Vec<String>` (re-parses, typechecks in Safe, returns ir.constraints)
  - `SymbolicEngine::check_obligations(ir) -> Vec<SolverCheck>`
  - `obligation_to_smt(obligation) -> String`: builds QF_LIA, (declare-const ... Int), (assert ...), (assert (not ...)), check-sat, get-model.
  - `expr_to_smt(e: &Expr) -> String`: crude (Var, Literal, (op lhs rhs), declass inner, assume/assert inner, taint_source_..., else "true").
  - `collect_vars_from_smt`, `run_z3_obligation` (spawns z3 -in -smt2, parses sat/unsat).
- No proper bitvector support yet (uses Int, QF_LIA).
- Called from:
  - `lib.rs` tests
  - `evidence/mod.rs`: `let solver_checks = ...::check_obligations(&tainted);`

## 5. SMT Output Path
- Raw SMT in `SolverCheck.smt` and `solver.json` in bundles under `analysis/solver.smt2` or similar (via evidence).
- In `run_z3_obligation`, smt built and sent to z3 stdin.
- Evidence: written in `build_evidence_bundle` as `solver.json`.

## 6. Model Parsing Path
- On "sat": `model: Some(stdout)` (full z3 output including model).
- No structured parsing of model into values; raw in evidence.
- No replay in current code (to be added in Gate 7).

## 7. Evidence Bundle Solver Output Path
- `evidence/mod.rs`:
  - `solver_checks = SymbolicEngine::check_obligations`
  - Builds `solver_json`, writes `solver.json`
  - `solver_hash`
  - In manifest checks: solver status.
  - `analysis/solver.json`, SARIF from checks, report.md.
- Tamper detection via MANIFEST.sha256.

## 8. Known Limitations (from Prior Audit + Inspection)
- Uses Int (no bitvec widths) — violates "bitvector widths and overflow explicit".
- `expr_to_smt` incomplete for & * + in complex expr; often falls to "true".
- Variables declared unconstrained in some paths (collect_vars may miss defs).
- Prior bug class: Unconstrained `result = 30` despite `secret * 3 + masked` and constraints (result not bound to expression in SMT).
- No replay validation.
- Assumes QF_LIA, no wrapping/checked semantics explicit.
- generate_constraints re-parses source (fragile).
- Models not mapped back to source spans/vars reliably.
- In research lowering, assumes/bounds from AST, but solver separate.

## 9. Prior Bug Class Reproduction
From audit:
```
let result = secret * 3 + masked;
assert(result > 30);
```
SMT could have free `result` satisfying assert without linking to `secret`.

This must be impossible after Gate 7 fixes.

## 10. Files Involved
- frontend/mod.rs (AST)
- middle/mod.rs (typecheck, analyze, SymbolicEngine, expr_to_smt, obligation_to_smt, run_z3)
- evidence/mod.rs (integration, solver.json, sarif)
- lib.rs (tests, re-exports)
- examples/ (symbolic_*.anb)
- z3 external (via Command)

## Next Steps for Gate 7
- Faithful lowering with bitvec, proper expr binding.
- Replay.
- Specific SARIF rules.
- Integration + tests + A15.

**Evidence:** This map produced after `grep`, `find`, code reads, `cargo test`. Committed as part of Gate 7 work.
