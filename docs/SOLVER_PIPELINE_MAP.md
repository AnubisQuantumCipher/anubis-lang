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
  - `run_z3_obligation_with_smt(obligation, smt) -> SolverCheck`: builds `(set-logic QF_BV)`, `(declare-const ... (_ BitVec 64))`, `(assert ...)`, `(assert (not ...))`, check-sat, get-model.
  - `expr_to_smt_with_width(e, widths) -> String`: i64-exact QF_BV — `bvadd/bvsub/bvmul/bvand/bvor/bvxor`, `bvneg/bvnot`, shifts `bvshl`/`bvashr` (with a mod-64 shift-amount mask; `>>` is arithmetic, matching `i64::wrapping_shr`), and `bvsdiv`/`bvsrem` for `/`/`%` with a NON-ZERO divisor — a non-zero literal, or a variable a `requires(v != 0)`/`requires(v > 0)` guard proves non-zero (and the body never reassigns/shadows, so the guard holds at the division); signed comparisons `bvslt/bvsle/…`; literals `(_ bv<n> 64)`. Guarded by `is_int_modelable`/`is_bool_modelable`, which still refuse truncating casts, floats, and division by an unguarded/zero divisor — fail-closed. Every obligation query runs under a z3 time budget (`Z3_ARGS`: soft `-t` + hard `-T`); a query z3 cannot decide returns `unknown`, which is treated as FAIL for contracts (undecided ≠ proved).
  - `collect_vars_from_smt`, `run_z3_obligation` (spawns z3 -in -smt2, parses sat/unsat).
- Bitvector-based: 64-bit `(_ BitVec 64)`, signed-i64-exact (landed after this doc's original draft; see git `343bbc2`).
- Called from:
  - `lib.rs` tests
  - `evidence/mod.rs`: `let solver_checks = ...::check_obligations(&tainted);`

## 5. SMT Output Path
- Raw SMT in `SolverCheck.smt` and `solver.json` in bundles under `analysis/solver.smt2` or similar (via evidence).
- In `run_z3_obligation`, smt built and sent to z3 stdin.
- Evidence: written in `build_evidence_bundle` as `solver.json`.

## 6. Model Parsing Path
- On "sat": `model: Some(stdout)` (full z3 output including model).
- Raw model kept in evidence; `parse_z3_model` (middle/mod.rs) structures it into var→BitVec value for replay.
- Replay IS implemented (see §8): `replay_counterexample(smt, model)` re-decides the ground formula.

## 7. Evidence Bundle Solver Output Path
- `evidence/mod.rs`:
  - `solver_checks = SymbolicEngine::check_obligations`
  - Builds `solver_json`, writes `solver.json`
  - `solver_hash`
  - In manifest checks: solver status.
  - `analysis/solver.json`, SARIF from checks, report.md.
- Tamper detection via MANIFEST.sha256.

## 8. Known Limitations (from Prior Audit + Inspection)
- Theory is QF_BV/64-bit only: no strings, floats, arrays/lists, quantifiers, or truncating casts; `<<`/`>>` and `/`/`%` are modeled (shifts always; `/`/`%` with a non-zero divisor — a literal, or a variable a `requires(v != 0)`/`requires(v > 0)` guard proves non-zero and the body never reassigns). Queries run under a z3 time budget so a hard symbolic division can never hang the checker — an undecided contract is failed closed. All unmodelable forms are refused fail-closed, not mismodeled.
- Variables declared unconstrained in some paths (collect_vars may miss defs).
- `replay_counterexample` (fixed 2026-07-11, commit `d25211f`) is a REAL model-substitution re-check: it parses z3's witness, pins every variable to it on top of the same `assumptions ∧ ¬assertion` query, and re-runs z3 — a genuine counterexample stays `sat`, a forged/inconsistent one goes `unsat` → `replay_valid: false`. Previously a substring stub that shipped a fabricated `solver_replay.json` attestation.
- Wrapping semantics ARE explicit now: `bvadd/bvsub/bvmul` match the i64 `wrapping_*` runtime exactly.
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
- middle/mod.rs (typecheck, analyze, SymbolicEngine, expr_to_smt_with_width, run_z3_obligation_with_smt)
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

## Hardening updates (this slice)
- Removed masked fallback hack.
- Per-var widths via symbolic_widths map, used in declare and literal adjust.
- Replay now detects hostile inconsistent models.
- SARIF rules for solver cases.
- clippy/fmt clean.
- u8 now BitVec 8 with correct lits.
