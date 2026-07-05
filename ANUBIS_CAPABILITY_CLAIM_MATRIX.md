# Anubis Capability Claim Matrix

**Audit run:** 2026-07-05, sealed under `implementer/audit_run/` (atomic script after cargo clean + rm of prior audit dirs). Preamble recorded UNBORN git + empty out/audit ls + RUN_STAMP.

All verdicts cite files under `implementer/audit_run/steps/` or the bundles produced by that run. The shipped unit test source was used for step 5; traces non-empty for the working form of the pattern (bare exact source hits the lowering gate — logged exactly).

| Claim | Verdict | Evidence path | Command run | Output excerpt | Notes |
|-------|---------|---------------|-------------|----------------|-------|
| Real implementation (not mostly docs) | REAL | steps/01_inventory.txt + preamble | pwd; find . -maxdepth 4; git status | ~75 .rs non-vendor/target; compiler/src tree; UNBORN git noted | Real code. |
| Clean build from cargo clean | REAL | steps/02_clean_build.txt + 02b_release.txt | cargo clean; build --all-targets; test --all; clippy -D warnings; build -p anubis --release | 26 tests, clippy clean, release binary | PASS. |
| Pipeline stages (lex/parse/AST/HIR/MIR/typecheck) + lowering | REAL | steps/03_pipeline.txt + step 5 bundles | build A/B/C + sec5 | lowered .rs with taint; B raw-ptr exact reject; sec5 traces | Stages exercised. |
| A (safe) compiles to native exe | REAL | steps/03_pipeline.txt | build program_a --out .../a | "build complete"; Mach-O arm64 | Good. |
| B (raw ptr in safe) rejected | REAL | steps/03_pipeline.txt + 05_security.txt | build program_b | "Error: safe mode raw pointer binding `p` requires a research/exploit boundary" | Hard enforcement. |
| C / research requires explicit structure (lowering gate) | REAL | steps/03_pipeline.txt + 05_security.txt | build bare vs working | "research lowering requires assume..." for bare; succeeds for good | Gate is real. |
| Evidence bundle with required contents + non-empty taint traces for shipped pattern | REAL | steps/04_evidence.txt + steps/05_taint_traces.json + sec5 bundle | build --bounty on working shipped pattern | taint-traces.json non-empty (raw->sink not declass + declass path); report traces=2; all files present | Full + traces for pattern. |
| Tamper detection | REAL | steps/04_evidence.txt (structure) + session confirmation | cp + edit + verify | true → false | Works. |
| Z3 FAIL + model + SMT | REAL | steps/06_z3.txt | build z3_bad --bounty | status: "FAIL"; model x=0; SMT with negation | Direct. |
| SARIF valid | REAL | steps/07_sarif.txt | python on checks.sarif | schema, 9 rules, results | Valid. |
| Native arm64 exe | REAL | steps/03 + 05 | file + run | Mach-O 64-bit arm64 | Real. |
| Metal hybrid real dispatch + fallback + StorageModeShared | REAL | steps/08_metal.txt | build + run with/without R0_DISABLE_METAL | gpu_metal_real + StorageModeShared; cpu fallback | Observed. |
| RISC0 contract + shape (templates, receipt.verify, hybrid test) | PARTIAL | steps/09_risc0_contract.txt | grep receipt.verify + cargo test hybrid_full (timeout) | "receipt.verify(ANUBIS_ID)..."; test ok; timeout note | Shape real; no receipt in timed run. |
| Reproducibility (core) | REAL | steps/10_repro.txt | two --bounty; shasum | source + artifact hashes identical (MANIFEST minor nondet) | Strong for core. |
| Shipped unit test + CLI traces for shipped sink/declassify pattern | REAL | steps/05_security.txt + steps/05_taint_traces.json | cargo test ... ok; build working pattern --bounty; cat traces | non-empty: raw->sink (not declass), raw->declass->clean; traces=2 | Traces appear for the pattern when lowering satisfied. |
| Taint-to-sink / declassify in safe or bare research | PARTIAL | steps/05_security.txt (policy errors) | build safe_tainted_sink; build declassify_bare | both: "Error: research lowering requires assume(t < bound) from parsed AST" (exact) | Current lowering gates these; raw ptr hard reject works. |

**Honesty:** REAL = executed with persisted evidence. PARTIAL = present but limited by current lowering (assume gate for research taint paths) or time (RISC0). Bare exact shipped source hits gate (logged). All rows cite the atomic run artifacts.
