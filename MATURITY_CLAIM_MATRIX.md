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
| Gate 10 RISC0 fresh receipt path | PARTIAL (real derived ImageID from risc0-build GUEST_ID + real Receipt.verify API wired + strict tamper on all sidecars + dev detection) | prove --backend risc0 , sidecars, verify cmd, A15 | cargo run -- prove ... --backend risc0 ; verify-receipt ; A15 log + tamper loops | real ID achieved, API call present, tamper strict, but full passing cryptographic receipt limited in this hybrid emit slice |

**Update rule:** After every material change, append or update row with new evidence path + exact command. A15 must be able to replay.

## Gate 2/3 Language Core Additions (2026-07-06 slice)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Gate 2 real language core (comments/fn/let/primitives/expr/control/structs/calls/builtins/attrs) | PARTIAL | 25 fixtures (20 PASS 5+ FAIL) + parser/AST/HIR/MIR + typecheck + runner + fresh a15 evidence with verdict FAIL for syntax/unknown/type/taint | bash scripts/run_language_fixtures.sh --out out/a15... ; jq . fixture_report.json ; cat */check-summary.json | grep verdict | comments // ; fn main typed params/return; let :u32/u8; arith + - * == < ; if/else ; while planned; struct lit/field; calls (user+builtin symbolic/assume/assert/taint_source/declassify/sink); @safe @research @proof @audit @effect parsed/preserved; no full modules/Result/enums yet |
| Gate 3 parser/AST/HIR/MIR maturity (spans, no panic, JSON emit, diags) | PARTIAL | --evidence produces *.ast.json *.hir.json *.mir.json ; parse_source_detailed + strict Err on diags; spans in AST/Stmt/Expr; clean diags for bad input | cargo run -- check f --evidence --out d ; find d -name '*.ast.json' ; grep -R ANUBIS_ d/ | never panics (lenient recovery + diags); file/line via spans; AST/ HIR/MIR JSON for ordinary workflows |
| Type checker + ANUBIS_* codes (unknown, mismatch, taint etc) | PARTIAL | ANUBIS_UNKNOWN_VARIABLE, ANUBIS_TYPE_MISMATCH, ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY emitted in check_error + bundle checks; fixtures enforce | cargo run -- check unknown... ; grep -R ANUBIS_ out/... ; same for type/taint/declass_missing | duplicate let / arity / cond type / missing return / field defined in typecheck; taint/declass rules unchanged |
| CLI ordinary workflow usable | PARTIAL | anubis check <f> ; build ; prove --backend risc0 ; verify-bundle ; verify-receipt ; doctor ; --evidence --out ; --emit ast,hir,mir | cargo run -- check ... ; cargo run --release -p anubis -- prove ... ; docs/CLI.md | check does not emit native by default; errors readable; doctor covers rust/risc0/metal/git/evidence |
| Fixture runner + golden + repro | REAL (scripts + tests) | scripts/run_language_fixtures.sh (EXPECT/ERROR_CONTAINS/ solver grep) + scripts/repro_language_core.sh ; fixture_report.json overall PASS; Rust unit tests in compiler for parser/type | bash scripts/run... ; jq -e '.overall_verdict == "PASS"' report ; bash scripts/repro... ; cargo test -p anubis-compiler language parser type | 25 fixtures; 5+ FAIL cases actual=FAIL in summaries; no masking |
| Language reproducibility (source + AST/diag hashes) | PARTIAL | repro script isolates timestamps; source+basic AST/diag stable for deterministic cases | bash scripts/repro_language_core.sh --out out/a_plus_gate2_repro ; jq . repro_report.json | nondet (ts) isolated; full deterministic for language checks in slice |
| Sealed gates preserved (4/5/7/8/10/11) | YES | regressions run with taint FAIL, symbolic solver, risc0 receipt verify, metal parity jq PASS, bundle verify PASS | see TASK 10 + A15 block in GATING; no gate weakened | language expansion did not touch sealed paths |
| General-purpose language complete | NO | explicit UNSUPPORTED + MATURITY | docs/language/UNSUPPORTED.md ; MATURITY_CLAIM_MATRIX | modules/imports full, enums/tagged, Result, large stdlib, async, networking, LSP, package publish out of this slice |
| Backend configuration portability (CLI/env/Anubis.toml/default) | REAL (this tranche) | resolve_metal_reference + doctor/prove wiring + evidence config_source | anubis doctor --metal-reference ... ; ANUBIS_...=... ; prove --metal-reference | All RISC0/Metal evidence now records how the reference was chosen; validators use recorded binding |
| `anubis doctor` (full) | REAL (expanded) | 20+ checks, --require-*, --evidence, JSON, verdicts | anubis doctor --require-risc0 --require-metal --json | Binary, git, rustc, RISC0 versions/paths/patch, Metal HAL, smoke, scripts, schemas |
| Release-candidate evidence builder | REAL (scripted) | scripts/build_release_candidate.sh + artifacts | bash scripts/build... --metal-reference ... --require-metal | Runs safety/fmt/test/clippy/fixtures/repro/doctor/gates + manifest + report |
| Install/version workflow | REAL (documented + binary) | docs/INSTALL.md + `anubis --version` + target/release/anubis | cargo build --release ; ./target/release/anubis --version ; doctor | Local install via symlink or direct binary use |
| Claim matrix honesty for local RC | REAL | This matrix + docs/CLAIMS.md + TRUST_BOUNDARIES etc. | grep REAL/PARTIAL/NOT CLAIMED | Local Apple Silicon + pinned reference = REAL for the listed surfaces; third-party / hosted / broad lang = NOT CLAIMED |
| Security language competitive radar (Gate 15) | REAL (this tranche) | docs/research/SECURITY_LANGUAGE_RADAR_2026.md with citations | (this doc) | CodeQL/Semgrep/KLEE/angr/libFuzzer/RISC0/Metal/etc. analysis + honest Anubis positioning |
| Security capability model + effect system (Gate 15) | PARTIAL (advancing) | parser attrs + mode from @ + effect checks (shell/file/network) + ANUBIS_* errors in middle + CLI | anubis check @research... ; run_security_fixtures | Parser attaches attrs, mode inference, enforcement for safe/poc/auth; more in progress |

## Probe + Core Tranche (2026-07-07)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Runtime probe capability evidence | REAL | `runtime-probe.json`, `RUNTIME_PROBE.md`, `MANIFEST.sha256` | `anubis runtime-probe --json --evidence --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover --out out/a16_runtime_probe` | Captures host/toolchain/RISC0/Metal reference identity and tree hashes; explicitly not proof execution |
| Runtime plan embeds probe truth | REAL PLAN-ONLY | `runtime-plan.json` with `runtime_probe`, `probe_hash`, `probe_status` | `anubis runtime-plan examples/risc0_receipt.anb --backend risc0 --lane metal-hybrid --apple-native --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover --json --evidence --out out/a16_runtime_plan_probe` | `status=plan-only`, `executed=false`; probe is capability evidence only |
| Ordinary safe `anubis run` | PARTIAL | `examples/hello_normal.anb`, `run-summary.json`, `stdout.txt`, `RUN.md` | `anubis run examples/hello_normal.anb --evidence --out out/a16_run_hello` | Supports first safe subset: let/literals/vars/arithmetic/comparisons/string concat/print/if/return; unsupported constructs fail closed |
| Runtime execution / planned-vs-observed enforcement | DEFERRED | future `runtime-exec.json` | future `anubis runtime-exec ...` | Next hard tranche; no current claim that runtime-plan executed a receipt path |

## Turing-Complete Executable Core (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| `while` / `loop` / `break` / `continue` execute | REAL | `tests/fixtures/turing_core/while_counter.anb` (→55), `loop_break.anb` (→110); `out/turing_core/report.json` | `bash scripts/run_turing_core_fixtures.sh --out out/turing_core` | Unbounded iteration lowered to native Rust loops in `anubis run` |
| Mutation (`x = expr;`) executes | REAL | `while_counter.anb`, `collatz.anb` (→111) | `./target/release/anubis run tests/fixtures/turing_core/collatz.anb` | `let` bindings emitted `mut`; assignment mutates state |
| Recursion + mutual recursion execute (real call stack) | REAL | `recursive_factorial.anb` (→120), `recursive_fibonacci.anb` (→55), `mutual_recursion.anb` (→true) | `bash scripts/run_turing_core_fixtures.sh` | Whole program lowered; each fn → Rust fn; recursion on Rust stack |
| Operator set `/ % != && || !` + unary `-`/`!` + `else if` | REAL | `parses_unary_and_extended_operators`, `parses_else_if_chain_and_recursion` tests; `ops` fixture | `cargo test -p anubis-compiler` | Short-circuit `&&`/`||`; parser + eval both covered |
| **Turing completeness (universality witness)** | REAL | `tests/fixtures/turing_core/turing_machine.anb` → `14`/`6`; cross-checked vs BB-3 constants S(3)=14 Σ(3)=6 and an independent reference simulator; `docs/language/TURING_COMPLETENESS.md` | `./target/release/anubis run tests/fixtures/turing_core/turing_machine.anb` ; `jq -e '.overall_verdict=="PASS"' out/turing_core/report.json` | TM simulator written in Anubis (two-integer-stack tape) halts with the known busy-beaver output |
| Turing-core gate (honest, no false-green) | REAL | `scripts/run_turing_core_fixtures.sh` compares stdout byte-for-byte to `.expected`; verdict derived, never default | `bash scripts/run_turing_core_fixtures.sh` → `Overall: PASS (8/8)` | Missing binary/expected/mismatch/nonzero-exit ⇒ FAIL |
