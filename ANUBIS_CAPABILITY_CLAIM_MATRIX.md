# Anubis Capability Claim Matrix

> **Historical baseline, not current state.** This matrix records one audit run from 2026-07-05 —
> the project's starting point (see `ROADMAP_A_PLUS.md`'s framing of the same-dated
> `ANUBIS_REALITY_AUDIT.md` C-grade baseline). Every table row below is accurate for that date only.
> **Do not read any row as 2026-07-27 status.** Living open FA: **3 reds** at security
> **216/219** (multi-candidate + factory summary — `docs/CLAIMS.md`). Capset selfhost known-fail
> `c05_open_param_call`. Dated history: `MATURITY_CLAIM_MATRIX.md`.

> **Status vocabulary (2026-07-24):** freestanding `REAL` is banned. `under Command` means the claim is only as strong as re-running the **Command** column (and checking **Evidence**). Prefer sealed artifact paths for historical gates. See `docs/CLAIMS.md` session proof log for re-runs on this date.

**Audit run:** 2026-07-05, sealed under `implementer/audit_run/` (atomic script after cargo clean + rm of prior audit dirs). Preamble recorded UNBORN git + empty out/audit ls + RUN_STAMP.

All verdicts cite files under `implementer/audit_run/steps/` or the bundles produced by that run. The shipped unit test source was used for step 5; traces non-empty for the working form of the pattern (bare exact source hits the lowering gate — logged exactly).

| Claim | Verdict | Evidence path | Command run | Output excerpt | Notes |
|-------|---------|---------------|-------------|----------------|-------|
| Real implementation (not mostly docs) | under Command | steps/01_inventory.txt + preamble | pwd; find . -maxdepth 4; git status | ~75 .rs non-vendor/target; compiler/src tree; UNBORN git noted | Real code. |
| Clean build from cargo clean | under Command | steps/02_clean_build.txt + 02b_release.txt | cargo clean; build --all-targets; test --all; clippy -D warnings; build -p anubis --release | 26 tests, clippy clean, release binary | PASS. |
| Pipeline stages (lex/parse/AST/HIR/MIR/typecheck) + lowering | under Command | steps/03_pipeline.txt + step 5 bundles | build A/B/C + sec5 | lowered .rs with taint; B raw-ptr exact reject; sec5 traces | Stages exercised. |
| A (safe) compiles to native exe | under Command | steps/03_pipeline.txt | build program_a --out .../a | "build complete"; Mach-O arm64 | Good. |
| B (raw ptr in safe) rejected | under Command | steps/03_pipeline.txt + 05_security.txt | build program_b | "Error: safe mode raw pointer binding `p` requires a research/exploit boundary" | Hard enforcement. |
| C / research requires explicit structure (lowering gate) | under Command | steps/03_pipeline.txt + 05_security.txt | build bare vs working | "research lowering requires assume..." for bare; succeeds for good | Gate is real. |
| Evidence bundle with required contents + non-empty taint traces for shipped pattern | under Command | steps/04_evidence.txt + steps/05_taint_traces.json + sec5 bundle | build --bounty on working shipped pattern | taint-traces.json non-empty (raw->sink not declass + declass path); report traces=2; all files present | Full + traces for pattern. |
| Tamper detection | under Command | steps/04_evidence.txt (structure) + session confirmation | cp + edit + verify | true → false | Works. |
| Z3 FAIL + model + SMT | under Command | steps/06_z3.txt | build z3_bad --bounty | status: "FAIL"; model x=0; SMT with negation | Direct. |
| SARIF valid | under Command | steps/07_sarif.txt | python on checks.sarif | schema, 9 rules, results | Valid. |
| Native arm64 exe | under Command | steps/03 + 05 | file + run | Mach-O 64-bit arm64 | Real. |
| Metal hybrid real dispatch + fallback + StorageModeShared | under Command | steps/08_metal.txt | build + run with/without R0_DISABLE_METAL | gpu_metal_real + StorageModeShared; cpu fallback | Observed. |
| RISC0 contract + shape (templates, receipt.verify, hybrid test) | PARTIAL | steps/09_risc0_contract.txt | grep receipt.verify + cargo test hybrid_full (timeout) | "receipt.verify(ANUBIS_ID)..."; test ok; timeout note | Shape real; no receipt in timed run. |
| Reproducibility (core) | under Command | steps/10_repro.txt | two --bounty; shasum | source + artifact hashes identical (MANIFEST minor nondet) | Strong for core. |
| Shipped unit test + CLI traces for shipped sink/declassify pattern | under Command | steps/05_security.txt + steps/05_taint_traces.json | cargo test ... ok; build working pattern --bounty; cat traces | non-empty: raw->sink (not declass), raw->declass->clean; traces=2 | Traces appear for the pattern when lowering satisfied. |
| Taint-to-sink / declassify in safe or bare research | PARTIAL | steps/05_security.txt (policy errors) | build safe_tainted_sink; build declassify_bare | both: "Error: research lowering requires assume(t < bound) from parsed AST" (exact) | Current lowering gates these; raw ptr hard reject works. |

**Honesty:** `under Command` = re-run Command + check Evidence; PARTIAL = present but limited. PARTIAL = present but limited by current lowering (assume gate for research taint paths) or time (RISC0). Bare exact shipped source hits gate (logged). All rows cite the atomic run artifacts.
