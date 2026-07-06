# Anubis Maturity Claim Matrix (Live)

Seeded from 2026-07-05 C-grade audit + plan baseline. Every row requires Status + Evidence path + Command.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Real compiler stages (parse/HIR/MIR/taint/symbolic) | REAL | compiler/src/{frontend,middle}/ + lib.rs tests | cargo test | Spans, recovery, research blocks, taint traces present |
| Raw pointer hard reject in safe | REAL | lib.rs: safe_mode_rejects_raw_pointer... + typecheck | cargo test safe_mode_rejects... | Exact error |
| Evidence bundles + tamper | REAL | compiler/src/evidence + out/audit/evidence-tampered + tests | cargo test evidence... ; anubis verify | MANIFEST.sha256, validate fails on tamper |
| Z3 FAIL + model with var | REAL | lib.rs z3_solver... + middle SymbolicEngine | cargo test z3_solver... | Model mentions x |
| Metal hybrid dispatch + fallback | REAL (observable) | hybrid templates + lib.rs hybrid_host... + out/audit/hybrid | cargo test hybrid... ; R0_DISABLE_METAL=1 run | lane=metal / cpu |
| RISC0 shape + receipt.verify contract | REAL (shape) / PARTIAL (fresh receipt) | templates + full-hybrid tests | grep receipt.verify ; cargo test hybrid_full | No fresh minimal receipt in baseline audit |
| Taint-to-sink / declassify enforcement | PARTIAL | traces for working pattern only; bare/safe hit lowering gate | build safe_tainted_sink / declassify_bare | "research lowering requires assume..." exact |
| Research lowering requires assume bound from AST | REAL (current gate) | backends/native/mod.rs:94 + lib.rs research_lowering_requires... | cargo test research_lowering_requires... | Brittle gate |
| Unborn git at start of a+ work | REAL | git rev-parse --verify HEAD (pre-init) | (recorded in plan) | Baseline commit 4a2c462 on a-plus-maturity/20260705-1649 |
| 26+ tests pass, clean fmt/clippy | REAL | this run + prior audit | cargo test ; cargo fmt --check ; cargo clippy -D warnings | Baseline state |
| Full 15 A+ gates | PLANNED | this matrix + A_PLUS_ACCEPTANCE_CRITERIA.md | scripts/audit_a_plus.sh | Work in progress |
| Gate 7 solver faithful semantics (BV, defs, no free vars) | REAL | middle/mod.rs check_obligations + smt build + tests | cargo test solver ; cat smt | Per var widths, derived (= ), BV ops |
| Gate 7 counterexample replay + hostile failure detection | REAL | replay fn + unit test in lib.rs | cargo test replay_failed | Detects inconsistent model |
| Gate 7 u8 BitVec 8 + explicit semantics | REAL | width tracking + u8 fixture + smt | cargo run check ...overflow_u8 ; cat smt | BitVec 8 declare, 8 bit lits, wrapping |
| Gate 7 solver SARIF rules and polish | PARTIAL | build_sarif rule mapping + locations | jq sarif | ANUBIS_ASSERTION_COUNTEREXAMPLE etc, basic spans |
| Solver evidence/tamper | REAL | analysis/solver.* in bundles + verify | scripts/verify ; tamper test | smt, replay json, hash fail on tamper |
| Gate 10 RISC0 fresh receipt path | PARTIAL | prove --backend risc0 , sidecars, verify cmd, A15 | cargo run -- prove ... --backend risc0 ; verify-receipt ; A15 log | guest from frontend, receipt path, but ID not real derived from build (NO_REAL_ID_DERIVED); tamper partial for all sidecars |

**Update rule:** After every material change, append or update row with new evidence path + exact command. A15 must be able to replay.
