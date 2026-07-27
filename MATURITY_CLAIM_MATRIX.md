# Anubis Maturity Claim Matrix (Live)

> **Living status is not this file.** Dated rows below are accurate for their own date only.
> **Single source of truth:** `docs/CLAIMS.md` § Known open issues (2026-07-27). Snapshot
> (GROK-MAAT round 8): security **228/228**; language **244/244**; stdlib **45/45**; capset
> **5/5**; formal PASS; native **681/0**. Disease across **eight+** classes. Green = **no KNOWN
> defects**, not no defects. D1–D4 + research auth bypass + unknown-attr fail-closed. Test-count
> rows stale.

> **Status vocabulary (2026-07-24):** freestanding `REAL` is banned. `under Command` means the claim is only as strong as re-running the **Command** column (and checking **Evidence**). Prefer sealed artifact paths for historical gates. See `docs/CLAIMS.md` session proof log for re-runs on this date.

Seeded from 2026-07-05 C-grade audit + plan baseline. Every row requires Status + Evidence path + Command.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Real compiler stages (parse/HIR/MIR/taint/symbolic) | under Command | compiler/src/{frontend,middle}/ + lib.rs tests | cargo test | Spans, recovery, research blocks, taint traces present |
| Raw pointer hard reject in safe | under Command | lib.rs: safe_mode_rejects_raw_pointer... + typecheck | cargo test safe_mode_rejects... | Exact error |
| Program-wide Safe/Research/Exploit aggregation | under Command (2026-07-25) | `program_mode` recursive join; explicit-Safe effective-mode rule; `safe_mode_program_gate.rs`; `formal/Anubis/ModeAggregation.lean` | `cargo test -p anubis --test safe_mode_program_gate`; `cargo test -p anubis --bin anubis program_mode_`; `bash scripts/run_formal_gate.sh` | Highest privilege wins independent of source order and nesting; ordinary run rejects before lowering. Lean proves the abstract lattice only; Rust tests cover the implementation traversal |
| Rejection evidence has one honest verdict | under Command (2026-07-25) | `build_rejected_evidence_bundle`; rejection integration tests | `cargo test -p anubis --test safe_mode_program_gate` | Failed check auto-emits; requested failed build emits `FAIL`, PCA tier `rejected`, no artifact or proof claim |
| Evidence bundles + tamper | under Command | compiler/src/evidence + out/audit/evidence-tampered + tests | cargo test evidence... ; anubis verify | MANIFEST.sha256, validate fails on tamper |
| Z3 FAIL + model with var | under Command | lib.rs z3_solver... + middle SymbolicEngine | cargo test z3_solver... | Model mentions x |
| Metal hybrid dispatch + fallback | under Command (observable) | hybrid templates + lib.rs hybrid_host... + out/audit/hybrid | cargo test hybrid... ; R0_DISABLE_METAL=1 run | lane=metal / cpu |
| RISC0 shape + receipt.verify contract | under Command (shape) / PARTIAL (fresh receipt) | templates + full-hybrid tests | grep receipt.verify ; cargo test hybrid_full | No fresh minimal receipt in baseline audit |
| Taint-to-sink / declassify enforcement | PARTIAL | traces for working pattern only; bare/safe hit lowering gate | build safe_tainted_sink / declassify_bare | "research lowering requires assume..." exact |
| Taint qualifier is structured (not substring); index/field-access no longer launders taint (Phase-3 slice, 2026-07-11) | under Command (for the two pieces) / PARTIAL (flow analysis) | `ty::is_tainted` (anchored `tainted<` incl. container-nested) replaces `.contains("tainted")` substring; `expr_taint_source` gains `Index`(base OR index) + `FieldAccess`(base) arms | `cargo test -p anubis-compiler taint is_tainted`; `anubis check` on `sink(tainted.field)`/`sink(t[i])` → `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` | Fixes `TaintedRecord` false-positive + closes the index/field fail-open laundering. 4 adversarial rounds (1 caught a nested-container security regression pre-ship; 2-3 proved flow-sensitive reassignment CLEARING needs branch-dataflow → reverted, deferred). Honest boundaries in UNSUPPORTED.md: taint is reassignment-INSENSITIVE, intra-procedural (Call args-only), whole-binding granularity. Corpus uses no `tainted<T>` so blast radius = 0 |
| Interprocedural RETURN-taint summary + cast propagation (Phase-3 slice 2, 2026-07-11) | under Command | monotone fixpoint `compute_tainting_fns` before per-function analysis; scope-aware `body_returns_taint`; `expr_taint_source` `Call` arm consults `ctx.tainting_fns`; new `Expr::Cast` arm | `cargo test -p anubis-compiler interprocedural_return_taint`; `anubis check` on `fn g(){return taint_source("s");} sink(g())` → `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` | Closes the `sink(get_secret())` fail-open (callee returns internal taint), incl. let-chains, casts, and transitive returns; declassify-before-return reads clean; scope-aware (lexical shadowing). 2 adversarial rounds (round 1 caught a shadowing FP + a cast fail-open, both fixed; round 2 clean). Deferred: arg→return & param→sink summaries, higher-order/indirect calls. Corpus byte-identical |
| Intra-procedural block-scoped taint (Phase-3 slice B, 2026-07-11) | under Command | `analyze_stmts` snapshots/restores lexical BindingInfo scope around `if`/`else`/loop/`@research`/`@exploit`/hybrid bodies via `restore_block_scope` (solver assumptions/`solver_int_vars` untouched) | `cargo test -p anubis-compiler block_scoped_shadowing`; `anubis check` on `let x=5; if c { let x=taint(); } sink(x);` → accept; sink of inner shadow → `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` | Closes the pre-existing fail-CLOSED false positive where a block-scoped shadow overwrote the outer binding's taint for the rest of the function. Real leaks (inner sink, outer-tainted sink) still reject. Adversarially verified with live `anubis check`. |
| Interprocedural param→sink summary (Phase-3 A1, 2026-07-11) | under Command | monotone fixpoint `compute_param_sinks` → `ctx.param_sinks`; call-site check emits `ANUBIS_INTERPROC_SINK` | `cargo test -p anubis-compiler interprocedural_param_sink`; `anubis check` on `fn log(x){sink(x);} log(taint_source("s"))` → `ANUBIS_INTERPROC_SINK` | Direct + transitive (wrap→log→sink) param flow; declassify-before-sink in callee and non-sunk params stay clean. Monotone + scope-aware. |
| Interprocedural param→return summary (Phase-3 A2, 2026-07-11) | under Command | monotone `compute_param_return_taint` → `ctx.param_return_taint`; Call arm of `expr_taint_source` uses it (known user fns only return-params taint the call) | `cargo test -p anubis-compiler interprocedural_param_return`; `anubis check` on `wrap(taint); sink(y)` → reject; `ignore(taint); sink(y)` → accept | Closes arg-conditional return-taint chains; fixes historical any-arg over-tainting of known user functions. |
| Effect clause `uses(...)` + declared-vs-inferred (Phase-3 C1+C2, 2026-07-11) | under Command (analysis) | `Item::Fn.effects` parsed in `parse_fn`; `ANUBIS_UNDECLARED_EFFECT` when inferred capability ⊈ declared; effect inference covers let-inits + nested args | `cargo test -p anubis-compiler uses_clause`; `anubis check` on `uses(net.send){ read_file(...) }` → `ANUBIS_UNDECLARED_EFFECT` | Absent `uses` skips the check. Executable I/O codegen is C3. |
| Executable governed I/O (Phase-3 C3, 2026-07-11) | under Command | `read_file`/`write_file`/`open`/`send`/`connect`/`time`/`rand` emit real Rust (`std::fs`/`std::net`/`std::time`); removed from `is_non_run_builtin` | `cargo test -p anubis-compiler governed_io`; goldens still pass | Additive only; programs without I/O emit unchanged. Shell/exec/sql remain non-run. |
| I/O ↔ taint wiring (Phase-3 C4, 2026-07-11) | under Command | `is_io_taint_source` (read_file/open/input/read_line); `is_sink` includes write_file/write/send | `cargo test -p anubis-compiler io_read_is_taint` | Undeclassified read→sink/write_file/send → `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`. |
| Verification lane + uses-gated Safe I/O (Phase-3 C5 complete, 2026-07-11) | under Command | `authorized_caps` + `safe_cap_allowed`; `--verified` / `@verified` / `#[verified]`; check combines imports | `cargo test -p anubis-compiler phase3_uses_authorizes`; `anubis check tests/fixtures/modules/phase3_io/main.anb --verified` | Declared uses authorizes write/net in Safe; verified requires uses; multi-file fixture phase3_io. |
| Field/element-granular taint (Phase-3 A3) | DEFERRED | whole-binding only (documented in UNSUPPORTED.md) | — | Precision, not fail-open; needs path-sensitive taint labels. |
| Interprocedural contract composition (Phase-3 D) | under Command (solver path) | `fn_contracts` + requires@ obligations + ensures assume at `let` call sites | existing contract/solver tests | No separate `ANUBIS_CALLEE_REQUIRES_UNMET` code; unmet requires = failed obligation. |
| Research lowering requires assume bound from AST | under Command (current gate) | backends/native/mod.rs:94 + lib.rs research_lowering_requires... | cargo test research_lowering_requires... | Brittle gate |
| Unborn git at start of a+ work | under Command | git rev-parse --verify HEAD (pre-init) | (recorded in plan) | Baseline commit 4a2c462 on a-plus-maturity/20260705-1649 |
| 649 tests pass, clean fmt/clippy | under Command | `out/a_plus_a15_frontdoor_20260724-154145/gate_report.json` + G3 log | `cargo test --all` ; `cargo fmt -- --check` ; `cargo clippy --all-targets -- -D warnings` | Reconciled Friday, July 24, 2026 (A15); RUST_MIN_STACK=16MiB for large clap CLI unit tests. **Stale count** — 2026-07-26 recount: 707 compiler tests + 142 CLI tests (~910 workspace total); re-run the Command for the current number rather than trusting either figure. |
| Full 15 unified gates | under Command | `out/a_plus_a15_frontdoor_20260724-154145/gate_report.json` → 15/15 PASS, 0 FAIL, 0 SKIP | `bash scripts/audit_a_plus.sh --out out/a_plus_a15_frontdoor_20260724-154145` | A15 re-seal Friday, July 24, 2026. G14 is VZ-isolated **34/34** (T9 included). Exact verdict in `A_PLUS_FINAL_REPORT.md` + A15 audit dir. |
| Gate 7 solver faithful semantics (BV, defs, no free vars) | under Command | middle/mod.rs check_obligations + smt build + tests | cargo test solver ; cat smt | Per var widths, derived (= ), BV ops |
| Gate 7 counterexample replay + hostile failure detection | under Command (fixed 2026-07-11 — was a fabricated attestation) | `replay_counterexample`/`parse_z3_model` (middle/mod.rs) + evidence/mod.rs `solver_replay.json` | `cargo test -p anubis-compiler real_counterexample_replay` ; `anubis check ... --evidence` then `cat */analysis/solver_replay.json` | Was a substring hack (`model.contains("x")`, hardcoded `#x0000000f`/`"15"`) shipping a fake `replay_valid` into every PCA bundle's `solver_replay.json`, with a tautological test. Now: parses z3's concrete witness, pins every variable to it on top of the SAME assumptions+¬assertion the solver decided, and asks z3 to re-decide the fully-ground formula — real model-substitution + re-execution, sound for QF_BV. Verified: prove gate 11/11, PCA gate 13/13 |
| Gate 7 u8 BitVec 8 + explicit semantics | under Command | width tracking + u8 fixture + smt | cargo run check ...overflow_u8 ; cat smt | BitVec 8 declare, 8 bit lits, wrapping |
| Gate 7 solver SARIF rules and polish | PARTIAL | build_sarif rule mapping + locations | jq sarif | ANUBIS_ASSERTION_COUNTEREXAMPLE etc, basic spans |
| Solver evidence/tamper | under Command | analysis/solver.* in bundles + verify | scripts/verify ; tamper test | smt, replay json, hash fail on tamper |
| Gate 10 RISC0 fresh receipt path | PARTIAL (real derived ImageID from risc0-build GUEST_ID + real Receipt.verify API wired + strict tamper on all sidecars + dev detection) | prove --backend risc0 , sidecars, verify cmd, A15 | cargo run -- prove ... --backend risc0 ; verify-receipt ; A15 log + tamper loops | real ID achieved, API call present, tamper strict, but full passing cryptographic receipt limited in this hybrid emit slice |

**Update rule:** After every material change, append or update row with new evidence path + exact command. A15 must be able to replay.

## Gate 2/3 Language Core Additions (2026-07-06 slice)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Gate 2 real language core (comments/fn/let/primitives/expr/control/structs/calls/builtins/attrs) | PARTIAL | 25 fixtures (20 PASS 5+ FAIL) + parser/AST/HIR/MIR + typecheck + runner + fresh a15 evidence with verdict FAIL for syntax/unknown/type/taint | bash scripts/run_language_fixtures.sh --out out/a15... ; jq . fixture_report.json ; cat */check-summary.json | grep verdict | comments // ; fn main typed params/return; let :u32/u8; arith + - * == < ; if/else ; while planned; struct lit/field; calls (user+builtin symbolic/assume/assert/taint_source/declassify/sink); @safe @research @proof @audit @effect parsed/preserved; no full modules/Result/enums yet |
| Gate 3 parser/AST/HIR/MIR maturity (spans, no panic, JSON emit, diags) | PARTIAL | --evidence produces *.ast.json *.hir.json *.mir.json ; parse_source_detailed + strict Err on diags; spans in AST/Stmt/Expr; clean diags for bad input | cargo run -- check f --evidence --out d ; find d -name '*.ast.json' ; grep -R ANUBIS_ d/ | Named malformed-input fixtures complete without panic (not a total parser-panic claim); file/line via spans; AST/HIR/MIR JSON for ordinary workflows |
| Type checker + ANUBIS_* codes (unknown, mismatch, taint etc) | PARTIAL | ANUBIS_UNKNOWN_VARIABLE, ANUBIS_TYPE_MISMATCH, ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY emitted in check_error + bundle checks; fixtures enforce | cargo run -- check unknown... ; grep -R ANUBIS_ out/... ; same for type/taint/declass_missing | duplicate let / arity / cond type / missing return / field defined in typecheck; taint/declass rules unchanged |
| Float→integer narrowing rejection (Phase-2 slice 1) | under Command | `ty::assignable`/`ty::is_float` (middle/ty.rs) wired at let-init/assign/arg/return; `ty::compatible` + `ty_parity` oracle unchanged | `cargo test -p anubis-compiler float_does_not_narrow` ; `narrowing_rule_does_not_reject` ; `assignable_rejects` | First rule consuming structured `Ty`. Directional: float→int rejected, int→float widening + width-interop kept. Faithful to runtime: bitwise/shift/`~`→int, float arith→float, if/match float only if every inferable branch float (adversary-verified: false positives on bitwise + mixed-branch found and fixed). Boundary: float via call-return/index/statement-if-in-block infers None → not yet narrowed (safe, documented in UNSUPPORTED.md) |
| CLI ordinary workflow usable | PARTIAL | anubis check <f> ; build ; prove --backend risc0 ; verify-bundle ; verify-receipt ; doctor ; --evidence --out ; --emit ast,hir,mir | cargo run -- check ... ; cargo run --release -p anubis -- prove ... ; docs/CLI.md | check does not emit native by default; errors readable; doctor covers rust/risc0/metal/git/evidence |
| Fixture runner + golden + repro | under Command (scripts + tests) | scripts/run_language_fixtures.sh (EXPECT/ERROR_CONTAINS/ solver grep) + scripts/repro_language_core.sh ; fixture_report.json overall PASS; Rust unit tests in compiler for parser/type | bash scripts/run... ; jq -e '.overall_verdict == "PASS"' report ; bash scripts/repro... ; cargo test -p anubis-compiler language parser type | 25 fixtures; 5+ FAIL cases actual=FAIL in summaries; no masking |
| Language reproducibility (source + AST/diag hashes) | PARTIAL | repro script isolates timestamps; source+basic AST/diag stable for deterministic cases | bash scripts/repro_language_core.sh --out out/a_plus_gate2_repro ; jq . repro_report.json | nondet (ts) isolated; full deterministic for language checks in slice |
| Sealed gates preserved (4/5/7/8/10/11) | YES | regressions run with taint FAIL, symbolic solver, risc0 receipt verify, metal parity jq PASS, bundle verify PASS | see TASK 10 + A15 block in GATING; no gate weakened | language expansion did not touch sealed paths |
| General-purpose language complete | NO | explicit UNSUPPORTED + MATURITY | docs/language/UNSUPPORTED.md ; MATURITY_CLAIM_MATRIX | modules/imports full, enums/tagged, Result, large stdlib, async, networking, LSP, package publish out of this slice |
| Backend configuration portability (CLI/env/Anubis.toml/default) | under Command (this tranche) | resolve_metal_reference + doctor/prove wiring + evidence config_source | anubis doctor --metal-reference ... ; ANUBIS_...=... ; prove --metal-reference | All RISC0/Metal evidence now records how the reference was chosen; validators use recorded binding |
| Cold-verify reproducible off the author's desk (no leaked home paths) | under Command | in-repo vendored `vendor/risc0-circuit-rv32im`; `DEFAULT_METAL_REFERENCE=""` → in-repo vendor; committed fixture `tests/fixtures/zk_prove_bundle` scrubbed of `/Users/sicarii/...` (metadata + generated-methods + manifest re-hashed); `run_prove_gate.sh` references nothing machine-specific | `grep -rc /Users/sicarii tests/fixtures/zk_prove_bundle` → 0; `bash scripts/run_prove_gate.sh` → 11/11 (self-contained cold-verify) | A stranger on Apple Silicon reproduces cold-verify from the repo alone. PRODUCING a new Metal receipt still needs an Apple-Silicon Metal environment (honest smaller claim) |
| `anubis doctor` (full) | under Command (expanded) | 20+ checks, --require-*, --evidence, JSON, verdicts | anubis doctor --require-risc0 --require-metal --json | Binary, git, rustc, RISC0 versions/paths/patch, Metal HAL, smoke, scripts, schemas |
| Release-candidate evidence builder | under Command (scripted) | scripts/build_release_candidate.sh + artifacts | bash scripts/build... --metal-reference ... --require-metal | Runs safety/fmt/test/clippy/fixtures/repro/doctor/gates + manifest + report |
| Install/version workflow | under Command (documented + binary) | docs/INSTALL.md + `anubis --version` + target/release/anubis | cargo build --release ; ./target/release/anubis --version ; doctor | Local install via symlink or direct binary use |
| Claim matrix honesty for local RC | under Command | This matrix + docs/CLAIMS.md + TRUST_BOUNDARIES etc. | grep 'under Command|PARTIAL|NOT CLAIMED|not claimed' | Local Apple Silicon + pinned reference = under Command for the listed surfaces; third-party / hosted / broad lang = NOT CLAIMED |
| Security language competitive radar (Gate 15) | under Command (this tranche) | docs/research/SECURITY_LANGUAGE_RADAR_2026.md with citations | (this doc) | CodeQL/Semgrep/KLEE/angr/libFuzzer/RISC0/Metal/etc. analysis + honest Anubis positioning |
| Security capability model + effect system (Gate 15) | PARTIAL (advancing) | parser attrs + mode from @ + effect checks (shell/file/network) + ANUBIS_* errors in middle + CLI | anubis check @research... ; run_security_fixtures | Parser attaches attrs, mode inference, enforcement for safe/poc/auth; more in progress |

## Probe + Core Tranche (2026-07-07)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Runtime probe capability evidence | under Command | `runtime-probe.json`, `RUNTIME_PROBE.md`, `MANIFEST.sha256` | `anubis runtime-probe --json --evidence --metal-reference $ANUBIS_RISC0_METAL_REFERENCE --out out/a16_runtime_probe` | Captures host/toolchain/RISC0/Metal reference identity and tree hashes; explicitly not proof execution |
| Runtime plan embeds probe truth | under Command PLAN-ONLY | `runtime-plan.json` with `runtime_probe`, `probe_hash`, `probe_status` | `anubis runtime-plan examples/risc0_receipt.anb --backend risc0 --lane metal-hybrid --apple-native --metal-reference $ANUBIS_RISC0_METAL_REFERENCE --json --evidence --out out/a16_runtime_plan_probe` | `status=plan-only`, `executed=false`; probe is capability evidence only |
| Ordinary safe `anubis run` | PARTIAL | `examples/hello_normal.anb`, `run-summary.json`, `stdout.txt`, `RUN.md` | `anubis run examples/hello_normal.anb --evidence --out out/a16_run_hello` | Supports first safe subset: let/literals/vars/arithmetic/comparisons/string concat/print/if/return; unsupported constructs fail closed |
| Runtime execution / planned-vs-observed enforcement | DEFERRED | future `runtime-exec.json` | future `anubis runtime-exec ...` | Next hard tranche; no current claim that runtime-plan executed a receipt path |

## Turing-Complete Executable Core (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| `while` / `loop` / `break` / `continue` execute | under Command | `tests/fixtures/turing_core/while_counter.anb` (→55), `loop_break.anb` (→110); `out/turing_core/report.json` | `bash scripts/run_turing_core_fixtures.sh --out out/turing_core` | Unbounded iteration lowered to native Rust loops in `anubis run` |
| Mutation (`x = expr;`) executes | under Command | `while_counter.anb`, `collatz.anb` (→111) | `./target/release/anubis run tests/fixtures/turing_core/collatz.anb` | `let` bindings emitted `mut`; assignment mutates state |
| Recursion + mutual recursion execute (real call stack) | under Command | `recursive_factorial.anb` (→120), `recursive_fibonacci.anb` (→55), `mutual_recursion.anb` (→true) | `bash scripts/run_turing_core_fixtures.sh` | Whole program lowered; each fn → Rust fn; recursion on Rust stack |
| Operator set `/ % != && || !` + unary `-`/`!` + `else if` | under Command | `parses_unary_and_extended_operators`, `parses_else_if_chain_and_recursion` tests; `ops` fixture | `cargo test -p anubis-compiler` | Short-circuit `&&`/`||`; parser + eval both covered |
| **Turing completeness (universality witness)** | under Command | `tests/fixtures/turing_core/turing_machine.anb` → `14`/`6`; cross-checked vs BB-3 constants S(3)=14 Σ(3)=6 and an independent reference simulator; `docs/language/TURING_COMPLETENESS.md` | `./target/release/anubis run tests/fixtures/turing_core/turing_machine.anb` ; `jq -e '.overall_verdict=="PASS"' out/turing_core/report.json` | TM simulator written in Anubis (two-integer-stack tape) halts with the known busy-beaver output |
| Turing-core gate (honest, no false-green) | under Command | `scripts/run_turing_core_fixtures.sh` compares stdout byte-for-byte to `.expected`; verdict derived, never default | `bash scripts/run_turing_core_fixtures.sh` → `Overall: PASS (11/11)` | Missing binary/expected/mismatch/nonzero-exit ⇒ FAIL |
| Arrays / lists (literal, index read/write, `len`, `push`, growable) | under Command | `tests/fixtures/turing_core/bubble_sort.anb` (→ sorted), `array_dp.anb` (→377/15) | `./target/release/anubis run tests/fixtures/turing_core/bubble_sort.anb` | `AnubisValue::List`; dynamic typing; enables real algorithms |
| `for v in a..b` range loops | under Command | `tests/fixtures/turing_core/for_range_sum.anb` (→5050) | `bash scripts/run_turing_core_fixtures.sh` | Desugars to counted while; bound evaluated once |
| Struct-literal-in-header ambiguity resolved (parser hang fix) | under Command | `header_position_is_not_a_struct_literal` unit test; `while running {}` / `for i in 0..n {}` | `cargo test -p anubis-compiler header_position` | Rust-style `no_struct` flag; also fixed latent `if flag {}` bug |
| `anubis run` wall-clock budget (fail-closed on runaway/non-terminating programs) | under Command | `run_child_capped_kills_runaway_native_binary` (real compiled spinner SIGKILLed inside budget), `run_child_capped_returns_output_when_program_is_fast`, `run_timeout_policy_defaults_and_opt_out`; `run.rs::run_child_capped` + `resolved_run_timeout` | `cargo test -p anubis-compiler run_child_capped run_timeout_policy` | Default 3600s (work-class invariant); `ANUBIS_RUN_TIMEOUT_SECS` override, `0` disables. Closes an observed orphaned-process leak: an infinite-loop program used to hang `anubis run` and, if the parent died, leak a CPU-pinning child. Direct child only. See `docs/language/UNSUPPORTED.md`. |

## Bounty-Grade PoC Kit (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Packing builtins (`p8`/`p16`/`p32`/`p64`, `cyclic`, list concat) | under Command | `examples/security/poc_packing_smoke.anb` → `16/65/65` | `bash scripts/run_poc_kit_gate.sh` | Runs inside disposable Tart/VZ guest; requires `--allow-research` there |
| Local process harness (`target_run`) | under Command | `examples/security/poc_local_overflow.anb` crashed=1 against guest-local `poc_kit/bin/vuln_local` | `anubis vz exploit --base anubis-xcode … --allow-research` | Same harness power, VZ-only; network URLs rejected |
| Gold local crash PoC | under Command | `poc_kit/vuln_local.c` + PoC fixture | `bash scripts/run_poc_kit_gate.sh` | Host orchestrates disposable guest; intentional lab oracle (abort if len>64) |
| Mutation process fuzz (real crashes) | under Command | guest `fuzz_report.json` engine=`mutation-process-v1`, unique crash bins | `anubis vz fuzz --base anubis-xcode poc_kit/bin/vuln_local --iterations 50 --allow-research` | Mutator unchanged; crash-capable execution VZ-only |
| Security fixture runner needle honesty | under Command | EXPECT FAIL + ERROR_CONTAINS requires needle in log; wrong failure ≠ green | `bash scripts/run_security_fixtures.sh` + `security_fixture_matches` unit tests | Fixed inverted-needle false-green |
| Network targets forbidden | under Command | fuzz/target_run reject `://` | gate fixture `network_forbidden` | Fail-closed dual-use boundary |
| PoC kit gate | under Command | `out/poc_kit/report.json` / gate script | `bash scripts/run_poc_kit_gate.sh --out out/poc_kit` | 4/4 packing + crash PoC + fuzz + network deny |
| Full unscoped malware platform | **NOT CLAIMED** | docs/language/OFFENSIVE_PLATFORM.md | — | Engagement-scoped red-team platform only |

## Offensive Platform AOP T1–T7 (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Engagement scope fail-closed | under Command | engage-init/status | `anubis engage-init` | kill date + authorization |
| aop-2 AES-GCM encrypted beacons | under Command | live whoami result protocol aop-2 | gate `t1_encrypted_c2` | PSK in engagement |
| Agent keys + jitter | under Command | agent meta key_id + jitter_pct | `agent-generate` | sleep jitter in agent |
| mTLS cert material | under Command | `certs/{ca,server,client}.{crt,key}.pem` | engage-init | CA+server+client |
| Full rustls mTLS handshake | under Command | `listen --mtls` + client cert `/health` | gate `t1_mtls_rustls` / local smoke | HTTP remains default |
| HTTP C2 + operator console | under Command | `GET /` HTML | gate `t7_console` | RBAC roles |
| Multi-operator token auth | under Command | `token_hash` + issue/revoke | gate `t7_operator_token_auth` | cleartext never stored |
| DNS + UDS transports | under Command | listen log multi-transport | gate t3_* | lab DNS/UDS |
| DNS/DoH C2 codec (`aop-dns-v1`) | under Command | `/doh` + UDP TXT | gate `t3_dns_doh_codec` | QNAME base32 + DoH |
| LaunchAgent persistence | under Command | `persistence/*.plist` | `persist-launchagent` | install script included |
| Inject plan-only (default) | under Command | PLAN_ONLY JSON | `inject-plan` | no silent inject |
| Live inject under double authorization | under Command | EXECUTED + loot/inject | gate `t2_inject_live_double_auth` | CLI flag + engagement half |
| Lateral SSH scoped | under Command | external deny | `lateral-ssh` | allowed_lateral_hosts |
| ROP pattern/gadgets/browser | under Command | pattern-offset found | pattern-*/gadget-*/browser-harness | lab browser localhost only |
| XOR packer | under Command | packs/*.xor.pack | `pack-xor` | lab packer |
| Exploit modules + PoC kit | under Command | exploit success | exploit-run | crash oracle |
| Offensive gate | under Command | gate report + isolation | `bash scripts/run_offensive_platform_gate.sh` | VZ-isolated host entrypoint; deeper surfaces gated |
| Host forbids AOP red-team execution | under Command | `ANUBIS_OFFENSIVE_HOST_FORBIDDEN` | listen/inject/lateral without VZ | Apple Virtualization guest required |
| PoC kit host entry is orchestration-only | under Command | packing + target_run + fuzz run in disposable guest | `run_poc_kit_gate` 4/4 + isolation.json | Gate refuses host crash execution (safety, not adversarial — env markers are user-settable); canonical `vz exploit`/`fuzz` path adds HMAC-validated run capability for defense-in-depth |
| All fuzz requires VZ | under Command | `ANUBIS_FUZZ_HOST_FORBIDDEN` | any direct host fuzz | No gold-fixture exception |
| ATT&CK kill-chain catalog (T9) | under Command | `aop-attck-v1` | `anubis attck-catalog --json` | Mapped to AOP surfaces |
| OPSEC score (T9) | under Command | `aop-opsec-v1` | `anubis opsec-score --engage …` | Elite checklist |
| Malleable C2 profile (T9) | under Command | profiles/*.json | `malleable-init` / `malleable-validate` | Lab traffic shaping |
| Campaign playbook (T9) | under Command | campaigns/full_spectrum.{json,md} | `campaign-init` | Full-spectrum phases |
| Purple-team report (T9) | under Command | purple_report.md | `purple-report` | ATT&CK coverage + gaps |
| Phish / LOLBAS PLAN_ONLY (T9) | under Command | executed:false | `phish-plan` / `lolbas-catalog` | Never auto-sends / auto-execs |
| Recon scoped (T9) | under Command | hostinfo host; scan VZ-only | `recon-hostinfo` / `recon-scan` | Fail-closed scope |
| SMB/WinRM lateral **execution** | NOT CLAIMED | `lateral-smb` CLI | `anubis lateral-smb --host …` | **PLAN_ONLY**: structured plan, `executed=false`, no SMB sockets |
| RBAC queue + admin status | under Command | listener `/task` + `/admin/status` + `task-queue --operator` | gate `t7_rbac_queue` | `role_can_queue` / `role_can_admin` wired |
| Structured `allowed_targets` | under Command | engage-status + scope | gate `scope_targets` | Host/Cidr/LocalPath kinds |
| String scramble (lab) | under Command | packer + `string-scramble` | gate `t6_string_scramble` | XOR note helper, not crypto |
| ANBP proof-input blob magic | under Command | `proof_input.anbp` + metadata | prove --evidence | magic `0x414E4250`, header validated |
| Security fixture honesty contract in doctor | under Command | offensive-doctor JSON | gate `doctor_t17` | rejects false-green needle pattern |
| Agent standalone cargo project | under Command | `[workspace]` empty in agent Cargo.toml | gate `t1_agent_encrypt` | no parent-workspace collision |

## RISC0 parameterized proofs (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Program-bound guest (input-free) | under Command | factorial journal 120, fib 55, distinct ImageIDs | `bash scripts/run_proof_binding_gate.sh` | commit 164488a |
| `proof_input_u32` / guest `env::read` map | under Command | guest contains `anubis_load_proof_inputs` + lookup | prove `examples/proof/proof_factorial_input.anb` | ABI v1 |
| CLI `--input-json` / `--input-file` | under Command | input_sha256 in metadata | prove with `--input-json '{"n":5}'` | exclusive flags |
| Same program, different inputs → different journals | under Command | n=5→120, n=6→720 | out/proof_factorial_5 + _6 | same ImageID |
| Same program → same ImageID | under Command | ImageIDs equal across n=5/n=6 | metadata compare | program-bound |
| input_sha256 + parameterized metadata | under Command | risc0_metadata.json schema 1.3 | prove --evidence | canonical JSON hash |
| Receipt verify for parameterized | under Command | verify_status=passed, !dev_mode | both n=5 and n=6 | Metal ref required |
| Parameterized proof gate | under Command | scripts/run_parameterized_proof_gate.sh | `bash scripts/run_parameterized_proof_gate.sh` | opt-in ~1–2 min |

## RISC0 Proof Bound to the Program (2026-07-09)

Retires the biggest honesty debt: `prove --backend risc0` previously proved a HARDCODED `x*6`
circuit on input `77`, decoupled from the input `.anb`. It now compiles the actual Anubis program
into the guest, so the ImageID (derived from that guest ELF) binds the receipt to the program.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| RISC0 guest compiled from the Anubis program (not a fixed circuit) | under Command | `out/proof_factorial/backend/risc0/guest/src/main.rs` contains `anb_factorial`/`anb_main`/`env::commit`; `risc0_metadata.json` `guest_binding=anubis-program` | `bash scripts/run_proof_binding_gate.sh` | `lower_program_to_guest` reuses the Turing-complete lowering; `std` guest |
| Receipt proves the program's real result (journal = computed value) | under Command | `proof_factorial` journal `120` = factorial(5); `proof_fib` journal `55` = fib(10); both `verify_status=passed`, `dev_mode=false`, `mock_prover=false` | `bash scripts/run_proof_binding_gate.sh` → `Overall: PASS` | journal decoded u32 LE; verified via `Receipt::verify(image_id)` |
| Proof is program-bound (different program → different ImageID) | under Command | factorial ImageID `2358913413…` ≠ fib ImageID `4137336513…` | `bash scripts/run_proof_binding_gate.sh` (distinct-ImageID check) | ImageID = cryptographic commitment to the compiled program |
| Real derived ImageID + real `Receipt::verify` + strict non-dev | under Command | `risc0_metadata.json` (real u32x8 ImageID, `image_id_is_placeholder=false`); `verify-receipt` re-extracts journal | `anubis verify-receipt --receipt … --image-id …` | bound to vendored patched `risc0-circuit-rv32im` at the reference path |

## Gate 11 Metal CPU vs Metal-hybrid parity (2026-07-09 honesty re-seal)

Retires same-dir / sealer-`|| true` / trivial-guest debt. Fixtures are program-derived (return `42` /
`x*6`); CPU and Metal prove into **distinct** `*_cpu` / `*_metal` dirs; sealer requires
`paths_distinct` and is fail-closed under `--require-metal`.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Distinct per-lane out dirs (no same-path compare) | under Command | sealer `parity.paths_distinct=true` on all 3 fixtures; script path check | `bash scripts/check_metal_parity.sh --require-metal --out out/a_plus_gate11_parity_continue` | A15: `implementer/a_plus_audit_run/20260709-095128/gate11_metal_parity` |
| CPU lane observed = `cpu` | under Command | all fixtures `cpu.lane_observed=cpu` + `R0_DISABLE_METAL=1` | same | not inferred from host only |
| Metal-hybrid lane observed = `metal-hybrid` | under Command (local Tier-2) | all fixtures `metal.lane_observed=metal-hybrid` | same | host aarch64/macos; CI Metal **NOT CLAIMED** |
| Journals match (program commit = 42 LE) | under Command | all 6 `journal.bin` = `2a000000`; sha256 `e8a4b2ee…d7cc` | same | extracted journals, not hardcoded |
| ImageID match per fixture (both lanes) | under Command | `image_id_match=true` per fixture | same | same guest ELF per program |
| Different programs → different ImageIDs | under Command | hello ≠ arithmetic ≠ symbolic_safe ImageIDs | jq fixtures | program-bound guests post Gate 10 binding |
| Both receipts verify | under Command | `receipt_verify=passed` both lanes | same | real `Receipt::verify` |
| Sealer fail-closed + A15 no `\|\| true` | under Command | `seal_rc=0`, `overall_verdict=PASS`; `gate11_a15_reproduce.sh` exits nonzero on fail | `./target/release/anubis gate11-metal-parity … --require-metal` | sealer exit no longer ignored |
| Overall Gate 11 under `--require-metal` | under Command (local Apple Silicon) | `overall_verdict=PASS` | full checker + sealer | third-party / hosted CI Metal still **NOT CLAIMED** |

## Multi-field journals (2026-07-09)

Public outputs beyond a single u32: `return [a, b, …]` commits each field via
`anubis_commit_journal` (scalar path remains v1-compatible).

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| List return → multi-u32 journal | under Command | a=3,b=4 → journal `[7,12]` (8 bytes) | `bash scripts/run_multi_field_journal_gate.sh` | `proof_multi_field.anb` |
| Different multi-field inputs → different journals, same ImageID | under Command | a,b=(3,4) vs (5,6) → `[7,12]` vs `[11,30]` | same gate | program-bound |
| Scalar journal still 4-byte u32 | under Command | factorial n=5 → 120 | same gate regression | no break of parameterized path |
| Private witness / redacted input split | NOT CLAIMED | inputs already not in journal (env::read only) | — | no selective redaction metadata yet |
| Named journal fields (`proof_commit_u32`) | under Command | `journal_fields` + `journal_decoded.json` | `bash scripts/run_named_journal_gate.sh` | names from guest source; values from journal |
| Host journal decode (u32 LE sequence) | under Command | `decode_journal_u32s` / `journal_fields_json` | same gate | synthetic `field_N` if unnamed |
| `proof_assert` fail-closed in guest | under Command | out-of-range fails prove/run | `examples/proof/proof_assert_range.anb` | private x not in journal names |
| `proof_commit_bool` | under Command | ok=1 in journal_fields | power gate | 0/1 public bit |
| Engagement action receipt chain | under Command | `evidence/receipts/chain.jsonl` + tip | `anubis receipt-verify` | hash-chained; tamper fail-closed |
| Power gate (language+proof+receipts) | under Command | `scripts/run_power_gate.sh` | bash gate | compound capability seal |
| Enums (`enum` + unit/tuple variants) | under Command | `examples/enum_status.anb` | `anubis run` → 42 | `Status::Err(42)` |
| `match` expressions + bindings | under Command | same + `proof_enum_status.anb` | `bash scripts/run_enum_match_gate.sh` | `_` wildcard supported |
| Struct-like enum variants (`Err { code }`) | under Command | `examples/enum_struct_variant.anb` → 99 | `bash scripts/run_lang_trio_gate.sh` | match named bindings |
| Maps / dictionaries `{k:v}` | under Command | `examples/map_dict.anb` → 6 | same gate | index get/set, for-in keys |
| if-expressions `let x = if c {a} else {b}` | under Command | `examples/if_expr.anb` → 7 | same gate | else required; else-if chains |
| Lang power trio (maps+struct-enum+if-expr) | under Command | `examples/lang_power_trio.anb` → 42 | same gate | combined executable surface |
| Prove if-expr + struct-enum + named journal | under Command | `examples/proof/proof_lang_trio.anb` | same gate (when metal ref present) | secret private; code+ok public |
| A+ call-site + let type checks | under Command | `a_plus_rejects_bool_for_u32_param` | `cargo test -p anubis-compiler a_plus_rejects` | `ANUBIS_TYPE_MISMATCH` / arity |
| A+ match exhaustiveness | under Command | `a_plus_match_non_exhaustive_fails_closed` | `cargo test -p anubis-compiler a_plus_match` | missing arms fail check; `_` OK |
| Hex/bin/oct integer literals | under Command | packing smoke uses `0x41414141` | `anubis run …/poc_packing_smoke.anb --allow-research` | lexer → decimal token |
| PoC `target_run` named TargetRun | under Command | `r.crashed` / `r.signal` … | `poc_local_overflow.anb` + list-compat `r[0]` | struct fields + index order |
| `for x in collection` list iteration | under Command | `examples/for_in_list.anb` → 60 | `bash scripts/run_for_in_gate.sh` | also turing fixture sum 15 |
| `for i in a..b` range (regression) | under Command | for_range_sum → 5050 | same gate | half-open |
| Prove for-in sum of private inputs | under Command | proof_for_in_sum journal 60 | same gate | a+b+c with proof_assert |

## Backend Unification Keystone (2026-07-10)

Retires the biggest structural debt: `build`/`prove` used a template/pattern-matched native emitter
(`backends/native/mod.rs::lower_to_native`) that faked execution (research template printing
`poc_memory_op_executed`; a `safe_execution` stub that never ran the program), while `run` used the
faithful whole-program transpiler. Now **every command shares `backends::run::lower_program_to_rust`** —
evidence, proof, and execution lower the *same* program.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| `build`/`prove` native artifact uses the faithful lowering (runs the real program, not a stub) | under Command | `build_of_program_with_main_emits_faithful_runnable_artifact` (lib.rs); CLI: `keystone-real-exec`/`42`, 0 `safe_execution` in emitted `.rs` | `cargo test -p anubis-compiler build_of_program_with_main` ; `anubis build FILE.anb --out d && d/anubis_out` | non-hybrid programs with `fn main` route through `lower_program_to_rust` |
| `run` vs `build` output parity (same program → same result) | under Command | run + build-artifact byte-identical on closures/map/reduce/for-in program | `anubis run p.anb` vs `anubis build p.anb && ./anubis_out` → `diff` identical | both pin `rustc --edition 2021` (main.rs:2829, native/mod.rs:40) |
| Non-runnable program (no `fn main`) → honest analysis-only marker (no fabrication) | under Command | `mainless_research_snippet_lowers_to_honest_analysis_marker`; CLI marker reports real taint `x: tainted<u32>`, `constraints: 4`, reason `no fn main()`, 0 `poc_memory_op_executed` | `anubis build examples/research_poc.anubis --out d && d/anubis_out` | reports mode/taint/constraints/reason; substance in evidence bundle |
| Brittle "research lowering requires assume(...)" gate RETIRED (honesty debt 0.3) | under Command | `research_snippet_without_assume_lowers_via_faithful_path_gate_retired` | `cargo test -p anubis-compiler research_snippet_without_assume` | now emits honest marker instead of a template gate error |
| Safe-mode enforcement preserved (no runnable artifact for violations) | under Command | raw ptr → `ANUBIS_RAW_POINTER_IN_SAFE`; tainted sink → `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`; both exit=1, no artifact | `anubis build <safe-with-rawptr>.anb` / `<safe-tainted-sink>.anb` | enforcement is in `typecheck`, upstream of lowering |
| Evidence bundles still validate after unification | under Command | safe + research bundles both `bundle valid: true` | `anubis build FILE --evidence --out d && anubis verify d/evidence-*` | tamper-evident hash chain intact |
| Hybrid programs unchanged (RISC0+Metal emitter) | under Command | `hybrid_host_compiles_and_dispatches`, `parses_hybrid_and_spec_blocks` green | `cargo test -p anubis-compiler hybrid` | `hybrid { }` still routes to `lower_hybrid`; known limit: only detected in the first fn (pre-existing) |
| Independent adversarial review (4 lenses, findings verified) | under Command | 1 low finding (edition mismatch) found+fixed; 2 dismissed as non-regressions | workflow `keystone-adversarial-review` | enforcement lens also proven firsthand via CLI |

## Language Correctness Sweep — Wave 1 (2026-07-10)

A 13-cluster adversarial cross-feature sweep (each finding verified firsthand) surfaced 32 confirmed
defects. Wave 1 fixed 13 (all high-severity + the number/crash classes), each with a pinned
regression test. Suite: **204 compiler tests, 0 failed** (`cargo test -p anubis-compiler`).

| Fix | Status | Test / evidence | Was |
|-----|--------|-----------------|-----|
| `as` cast binds tighter than binary ops (`300 as u8 + 1` = 45) | under Command | `cast_binds_tighter_than_binary_ops` | cast silently voided, `+1` dropped → 300 |
| Struct `==` is field-order-independent | under Command | `struct_equality_is_field_order_independent` | positional zip → `{x,y}` ≠ `{y,x}` |
| Named functions bind by bare name in `let` | under Command | `named_functions_bind_by_name_in_let` | `let f = double` → ANUBIS_UNKNOWN_VARIABLE |
| Compound assign evaluates a side-effecting index once | under Command | `compound_assign_evaluates_index_once` | `xs[pop(sel)] += 5` popped twice |
| Signed narrowing cast sign-extends (`255 as i8` = -1) | under Command | `integer_casts_and_wide_literals` | masked unsigned → 255 |
| Full-width radix literals (`0xFFFFFFFFFFFFFFFF` = -1) | under Command | `integer_casts_and_wide_literals` | i64 parse failed → 0 |
| `i64::MIN` decimal literal is exact | under Command | `integer_casts_and_wide_literals` | coerced to f64 |
| Named function passed to `map` pads (no panic) | under Command | `named_function_arity_pads_not_panics` | index-out-of-bounds panic |
| `assert`/`assume` work in expression position | under Command | `assert_and_assume_work_in_expression_position` | `Expr::Other` → unsupported-expr error |
| Empty `${}` interpolation is a clean diagnostic; empty `""` lowers | under Command | `empty_interpolation_and_empty_string_are_handled` | crash / `parts.remove(0)` panic |
| Built-in `Some/None/Ok/Err` render bare; maps show quoted keys | under Command | `display_forms_option_result_map_and_user_enum` | `Option::Some(x)`, `{a: 1}` |
| Actionable "unsupported expression" errors (no `Discriminant(N)`) | under Command | error arm split in `safe_run_expr` | opaque `Discriminant(28)` |

### Wave 2 (2026-07-10) — 5 more fixed

| Fix | Status | Test | Was |
|-----|--------|------|-----|
| Multi-arg generics (`Map<int,string>`, `Box<Box<T>>`) parse in annotations | under Command | `wave2_generics_patterns_and_try` | parse error at inner comma / `>>` |
| Duplicate struct-field literal rejected (`P { x:1, x:2 }`) | under Command | `duplicate_struct_field_is_rejected` | silently accepted, both stored |
| Or-pattern with wildcard (`Red \| _`) is exhaustive | under Command | `wave2_generics_patterns_and_try` | wrongly flagged non-exhaustive |
| `?` on a non-Option/Result/enum fails closed | under Command | `question_operator_respects_enum_type` (guard) | silently passed value through |

Suite after Wave 2: **206 compiler tests, 0 failed.** 18 defects fixed across both waves.

### Wave 3 (2026-07-10) — design decisions resolved

Two deferred design calls were decided by the operator:
- **Arity policy → strict direct, pad higher-order (COMPLETE).** A *direct* call with the wrong arity
  errors (`ANUBIS_ARITY_MISMATCH`); higher-order/pipeline use (`map`/`filter`) keeps padding. Enforced
  for **functions** (already), **methods** (`direct_method_call_arity_is_checked`; ambiguous same-name
  arities across impls left unchecked), and **closures / named-fn references**
  (`direct_closure_call_arity_is_checked`; arity tracked in the scope, recomputed on reassignment so a
  different-arity reassign does not false-positive; unknown-arity params left unchecked).
- **Map keys → keep string-keyed, documented.** LANGUAGE.md now states keys are strings and non-string
  indices coerce via display form (`m[5]`==`m["5"]`; `m[1]`≠`m[1.0]`). No code change.

Suite: **207 compiler tests, 0 failed.** 19 defects fixed + 2 design decisions resolved across three waves.

**Remaining tail (low-severity / follow-up — NOT mainline; do not clear before PCA):** mutating builtins
(`push`) on a struct-field place (clean fail-closed limitation, local-var workaround); struct display in
declaration vs insertion order (cosmetic); unknown-var in an `if` condition (coverage, false-positive
risk); low-severity edge cases (#23–32).

**Harness debt — RESOLVED (2026-07-10).** `run_language_fixtures.sh` now cleans each per-fixture dir
before running, so the `evidence-*` FAIL glob can't pick up a stale dir (deterministic PASS across
repeated runs, verified). `missing_semicolon.anb` was repurposed to pin the optional-semicolon feature
(EXPECT PASS); a new `unterminated_block.anb` preserves the negative parse-error coverage. Language
fixtures: **26/26 PASS.**

## PCA v0 — Proof-Carrying Artifact (2026-07-10)

The product of the thesis: an evidence bundle now carries a **claim block** (`pca.json`) — a
deterministic, source-derived verdict (mode, tier, parse/typecheck/taint/solver, `verdict`) — and
`anubis verify` **re-derives** it from the bundle's own source and cross-checks it, instead of merely
re-hashing recorded claims. A bundle whose recorded verdict disagrees with what the source actually
proves fails closed, even when every hash is recomputed to look internally consistent.

| Claim | Status | Evidence | Command |
|-------|--------|----------|---------|
| Claim block emitted into every evidence bundle | under Command | `pca.json` (`pca_version`, `source_sha256`, `mode`, `tier`, `parse_ok`, `typecheck_ok`, `taint_clean`, `solver_obligations`, `solver_all_discharged`, `verdict`) | `anubis check FILE --evidence --out d && cat d/evidence-*/pca.json` |
| Claim block is deterministic (re-derivable) | under Command | `derive_claim_block_is_deterministic`; no timestamp in the block | `cargo test -p anubis-compiler derive_claim_block` |
| `verify` re-derives the claim from source and cross-checks | under Command | `verify_pca` re-parses/typechecks/taint/solver the bundle's `source.anubis` and requires it to equal `pca.json` | `anubis verify d/evidence-*` |
| `verify` catches a lying claim even with consistent hashes | under Command | `verify_pca_rederives_claim_and_catches_a_consistent_lie` — forges `pca.json`, regenerates `MANIFEST.sha256` so the hash layer passes, yet `verify_pca` fails closed | `cargo test -p anubis-compiler verify_pca_rederives` |
| Tamper gate: source and claim tampering fail closed | under Command | 5/5 — claim block emitted + PASS verdict; clean verify passes; tampered source and tampered claim each fail closed | `bash scripts/run_pca_gate.sh` |

**PCA v0 scope (honest):** `tier` is `"checked"` (parse + typecheck + taint + solver ran) — this is a
verified *analysis* verdict, not yet a full T1/T2 assurance tier, and the block is not yet
cryptographically signed (the `manifest_signature` is still a SHA-256, not an asymmetric signature).
Next: real signing (Ed25519 + Secure Enclave), optional ZK-receipt binding, and tier grading.
Suite: **210 compiler tests, 0 failed.**

## PCA v0.1 — portable, attributable, prove-honest (2026-07-10)

Hardens PCA v0 into a portable product: a PCA built here verifies on another machine, with only the
bundle and a public key, without trusting the author — and the prove edge can no longer lie.

| Claim | Status | Evidence | Command |
|-------|--------|----------|---------|
| Prove fails closed when a program can't be lowered to a guest | under Command | echo-guest fallback removed; `ANUBIS_UNSUPPORTED_GUEST_LOWERING`, 0 receipts / 0 echo-guest files written | `anubis prove <no-fn-main>.anb --backend risc0` → error, no receipt |
| Portable / cold verify (no Metal, no prove path) | under Command | `verify` re-derives the claim with no Metal dependency | `ANUBIS_RISC0_METAL_REFERENCE=/nonexistent R0_DISABLE_METAL=1 anubis verify d/evidence-*` → valid |
| Claim block states `tier="checked"`, `zk_present=false` (no overclaim) | under Command | honest fields; source binding asserted explicitly in `verify_pca` | `jq '.tier,.zk_present' d/evidence-*/pca.json` |
| Ed25519 keygen / sign / attributable verify | under Command | `anubis keygen`, `anubis sign`, `pca.sig` (algorithm/public_key/signature over `sha256(pca.json)‖sha256(MANIFEST.sha256)`); `verify` reports `signed: <bool> (signer …)` | `anubis keygen --out k; anubis sign d/evidence-* --key k/signing.key; anubis verify d/evidence-*` |
| Unsigned PCA still valid; missing sig → `signed: false` | under Command | `verify` passes unsigned, prints `signed: false (unsigned PCA)` | `anubis verify <unsigned-bundle>` |
| Wrong / forged signature or `--pubkey` mismatch fails closed | under Command | `verify --pubkey <wrong>` → exit 1; tampering a signed claim invalidates the sig → exit 1 | `anubis verify d/evidence-* --pubkey <wrong>` |
| Tamper gate (cold + tamper + sign) | under Command | 13/13 | `bash scripts/run_pca_gate.sh` |

**Unit tests:** `verify_pca_rederives_claim_and_catches_a_consistent_lie`,
`sign_and_verify_pca_roundtrip_then_tamper_fails`, `derive_claim_block_is_deterministic`. Suite:
**211 compiler tests, 0 failed; PCA gate 13/13.** Deps added: `ed25519-dalek 2`, `getrandom 0.2`.

**Honest scope / next:** signing is software Ed25519 (file-based keys); Secure Enclave / hybrid ML-DSA
is a later upgrade of the same key story. `tier` is still `"checked"`. Next on the arc: optional
ZK-receipt binding into the claim block (re-verify a receipt against its ImageID when present; never
invent one), then tier grading toward T2.

## Fail-closed indexing (2026-07-10) — finish-the-language wave

The completeness audit flagged a core contradiction: a self-described *fail-closed* language silently
returned `0` on out-of-bounds list index, past-the-end string index, and missing map key. Fixed in
`index_get` (`compiler/src/backends/run.rs`, inside the emitted `ANUBIS_CORE_RUNTIME_RS`):

| Access | Before | After |
|--------|--------|-------|
| `xs[i]` list OOB | `0` | panic `ANUBIS_INDEX_OUT_OF_BOUNDS` (message names the length + points to `get`) |
| `s[i]` / `char_at` OOB | `""` | panic `ANUBIS_INDEX_OUT_OF_BOUNDS` |
| `m[k]` absent key | `0` | panic `ANUBIS_MISSING_KEY` (points to `get`/`has_key`) |
| index a non-collection | `0` | panic `ANUBIS_NOT_INDEXABLE` |

**Deliberately unchanged (documented contracts):** `list_elem` (destructuring keeps its not-a-list→0
contract), `field_get` (method/field-default→0), struct list-view compat (`r[0]` for TargetRun), and
the safe accessors `get(coll, key, default)` / `has_key(m, k)`. Negative indexing (`xs[-1]`) still valid.

**Evidence:** regression tests `index_out_of_bounds_fails_closed`, `missing_map_key_fails_closed`,
`safe_accessors_survive_fail_closed_indexing` (assert nonzero exit + trap message via
`compile_and_run_source`). Verified no regression: **215 compiler tests, 0 failed**; turing core 13/13;
language fixtures 26/26; PCA gate 13/13; language-core repro PASS; all 8 `examples/feel/*` dogfood
programs run (the two hand-written lexers guard every `char_at` with `i < n`, so trapping is safe).
LANGUAGE.md updated (indexing + maps sections). Re-run:
`cargo test -p anubis-compiler --release -- fail_closed index_out_of_bounds missing_map_key`.

## Enum-construction validation (2026-07-10) — closes two audit footguns at once

The audit flagged two fail-open holes that share one root cause: `Foo::Bar` parses to
`Expr::EnumConstruct { enum_name, variant, .. }`, and an EnumConstruct with an unregistered
enum/variant was never checked — it silently lowered to a stringy enum value. Two symptoms:
- **Undefined variant**: `Color::Purple` (with `enum Color { Red, Green, Blue }`) passed `check`.
- **Qualified-call footgun**: a Rust-style `math::double(21)` passed `check` and printed the literal
  string `"math::double(21)"` instead of calling — while an unknown *bare* call was already caught
  (`ANUBIS_UNKNOWN_FUNCTION`).

Fix (`check_calls_expr` in `compiler/src/middle/mod.rs`): validate every `EnumConstruct` against the
`enum_variants` registry (populated in pass-1 `register_program_surface`, recursing into modules).
Unknown enum type → `ANUBIS_UNKNOWN_ENUM` (message points at the flat call namespace); real enum but
absent variant → `ANUBIS_UNKNOWN_VARIANT` (lists the known variants). Both `check` and `run` fail
closed. Builtin `Option`/`Result` are pre-registered, so `Some`/`None`/`Ok`/`Err` still pass.

**Evidence:** `enum_construct_is_validated` (asserts both errors fire and that unit/tuple/recursive
enums + builtin `Ok` still type-check). **216 compiler tests, 0 failed**; language fixtures 26/26;
PCA gate 13/13; all 8 `examples/feel/*` run (enum-heavy `03_engagement_ledger` uses
`Verdict`/`Status`/`Review`). LANGUAGE.md enums section updated. Re-run:
`cargo test -p anubis-compiler --release enum_construct_is_validated`.

## `input()` / `read_line()` now functional (2026-07-10)

Documented stdin builtins returned empty. The runtime `anubis_input()` was correct, but the CLI `run`
path executed the compiled binary with `Command::output()`, which **closes the child's stdin** — so
every stdin read hit EOF. Fixed by forwarding the parent's stdin (`.stdin(Stdio::inherit())`) on the
run-spawn in `tools/anubis/src/main.rs`; stdout/stderr stay captured for the run evidence bundle.

**Evidence:** regression test `read_line_reads_stdin` (pipes `"40\n2\n"` -> prints `42`; `input()`
alias strips the newline) via a new `run_with_stdin` harness that spawns the binary with piped stdin.
Verified firsthand: `printf "10\n20\n12\n" | anubis run` -> `sum: 42`; a no-stdin program still runs
without hanging. **217 compiler tests, 0 failed.** Re-run:
`cargo test -p anubis-compiler --release read_line_reads_stdin`.

## Dogfood sweep — 15 domains, 6 confirmed bugs fixed (2026-07-10)

Ran a 15-domain "build real programs and run them firsthand" workflow (JSON parser, Dijkstra/
topo-sort/union-find, arbitrary-precision bignum, regex engine, Markdown→HTML, shunting-yard/RPN,
in-memory SQL, matrix algebra, calendar, sorting zoo, stdin text adventure, non-crypto hashes,
bytecode VM, cellular automata, heap+Huffman). **44 programs built, 43 ran correctly first-try;
7 domains completely clean** (regex, markdown, rpn-calc, sql, matrix, adventure, hashes). Every
finding below was reproduced firsthand before fixing, and each fix is gated + regression-tested.

| Bug (severity) | Root cause | Fix commit | Test |
|----------------|-----------|-----------|------|
| `get(list/str, i, default)` always returned the default (HIGH) | `anubis_get` only matched Map | `f3630f2` | `get_returns_element_for_lists_strings_and_maps` |
| `_` wildcard binding → `let mut _` rustc error (HIGH) | emitter always prefixed `mut` | `b0fefc2` | `wildcard_binding_lowers_without_mut` |
| `check` disproved `assert(true)` + fabricated string counterexamples (HIGH) | bool/string BV-encoded; z3 error treated as disproof | `d4d0cd9` | `solver_never_disproves_unmodelable_assertions` |
| deep recursion (~8500) aborted with raw Rust stack overflow (MEDIUM) | main ran on 8 MiB OS stack | `c88bddd` | `deep_recursion_runs_on_large_stack` |
| `match` as non-final stmt in closure/arm body failed to parse (MEDIUM) | block parser took `match` as the tail | `12aaa96` | `match_statement_in_closure_and_arm_bodies` |
| `parse_int`/`parse_float` fail-open (return 0 on malformed) (MEDIUM) | lenient by design; no checked variant | `7860917` (added `parse_int_opt`/`parse_float_opt`) | `parse_opt_returns_matchable_option` |

**Non-bugs surfaced + documented (not code changes):** list/map/struct arguments pass **by value**
(mutating a parameter doesn't reach the caller; return-and-reassign is the idiom) — flagged by 6+
agents as under-documented, now a dedicated LANGUAGE.md section with a verified example. Lenient
`parse_int`/`int` behavior documented alongside the new fail-closed `*_opt` variants.

Net: **224 compiler tests (was 212 at sweep start), 0 failed**; language fixtures 26/26; PCA 13/13;
turing 13/13; all 8 `examples/feel/*` run. Sweep transcript: workflow `wf_d8a05288-c83`.

## Dogfood sweep round 2 — 15 harder domains, 2 confirmed bugs fixed (2026-07-10)

Second round on the fixed binary (Lisp interpreter, Brainfuck, Sudoku solver, CSV query pipeline,
exact fractions, trie, LRU+Bloom, LCS diff, base64/URL, template engine, A* pathfinding, lambda
calculus, big-decimal, state machine, JSONPath). **~40 programs, ~all ran first-try; 13 of 15 domains
completely zero-defect** — cleaner than round 1, confirming the round-1 fixes hold in real programs.
Two confirmed HIGH bugs, both fixed:

| Bug (severity) | Root cause | Fix commit | Test |
|----------------|-----------|-----------|------|
| `check` disproved a TRUE loop-carried assertion (`for..{ total=total+i } assert(total==10)`) (HIGH) | reassigned var kept its stale pre-loop concrete assumption | `f20ad27` | `solver_does_not_disprove_loop_carried_assertions` |
| research words (`unified`/`symbolic`/`cpu`/`gpu`/`prove`/…) reserved → rejected as ordinary identifiers / user fn names on the run path (HIGH) | global lexer keywords | `33196ae` | `research_words_usable_as_identifiers` |

**Deliberately not changed:** deep metacircular recursion (~20k interpreter levels ≈ ~200k heavy native
frames) can still exhaust the 1 GiB stack and abort — inherent (Rust can't catch stack overflow), and
1 GiB is already generous (direct recursion succeeds at 500k). Documented limitation, not a fix target.

Also added `is_empty` (many agents hand-wrote `len(x) > 0`). Net across rounds 1+2: **227 compiler
tests, 0 failed**; fixtures 26/26; PCA 13/13; POC kit 4/4; proof-binding PASS. Round-2 transcript:
workflow `wf_7abc8eda-302`. A third round (15 more domains) is running to confirm the tail is dry.

## Dogfood sweep round 3 — DRY (15/15 zero-defect) + the O(n²) indexing fix (2026-07-10)

Third round on the fixed binary across 15 entirely new hard domains (minimax game AI w/ alpha-beta,
spreadsheet engine w/ dependency recalc, ray tracer, Verlet physics sim, memoizing PEG parser, π to
30-50 digits via Machin+spigot bignum, LZ77/LZW compressors, Thompson NFA→DFA regex, dimensional-units
calculator, cron scheduler, genetic algorithm, KV store w/ WAL replay, roman/base-N numerals, skip
list, Turing machine). **~53 programs built, ~all ran first-try; 15 of 15 domains ZERO-DEFECT — zero
confirmed and zero suspected language bugs.** This is the loop-until-dry convergence signal: after 8
fixes across rounds 1–2, a fresh 15-domain round finds nothing. The executable core is mature for the
niche.

The one recurring theme (spreadsheet, pi-digits, peg-parser agents each pinpointed it) was
**performance, not correctness**: indexed reads `a[i]` and field reads `p.f` on a local routed through
`var_as_value`, which cloned the WHOLE collection/struct (`(a.clone()).index_get(i)`) — so an indexed
read in a loop was O(n) and in-place array algorithms went O(n²). Fixed by borrowing the local directly
(`a.index_get(i)` / `p.field_get("f")`; both take `&self` and clone only the element/field):

| Fix | commit | Evidence |
|-----|--------|----------|
| `a[i]` on a local borrows instead of cloning the collection | `caec2e8` | 2500-elem insertion sort: **>5 min (timed out) → 0.89 s**; test `indexed_reads_fast_path_is_correct` |
| `p.f` on a local borrows instead of cloning the struct | `d76464b` | field read is copy-not-alias; 228 tests |

Value semantics are unchanged (the element/field is still copied out). **228 compiler tests, 0 failed;
fixtures 26/26; PCA 13/13; all 8 examples/feel run.** Round-3 transcript: workflow `wf_d6f70170-634`.
Residual perf notes (not fixed, lower priority): map `m[k]` is still an O(n) linear scan (association-
list representation, not a hashmap); generated Rust is compiled without `-O`.

**Dogfooding phase: CONVERGED.** 3 rounds, 45 domains, ~150 real programs; 8 correctness bugs found +
fixed in rounds 1–2, round 3 dry. The language runs substantial real software (interpreters, solvers,
parsers, data structures, numerics, simulations) correctly and now performantly for in-place algorithms.

## ZK Receipt Binding — Track A (2026-07-10, commit 3cc4779)

The Proof-Carrying Artifact now binds a real RISC Zero receipt and re-verifies it cold. All firsthand
on this M4 Max; nothing simulated.

**A0 — substrate proven (firsthand, hard gate).** `anubis prove --backend risc0 --lane metal-hybrid`
on `examples/proof/proof_factorial_input.anb` produces a GENUINE receipt in ~28s: 221 KB bincode
receipt, `dev_mode=false`, `mock_prover=false`, `image_id_is_placeholder=false`,
`lane_observed=metal-hybrid`, correct journal (n=5→120, n=6→720), program-bound deterministic ImageID
(same across inputs). `verify-receipt` re-verifies (exit 0) and fails closed on a wrong ImageID
("claim digest does not match", exit 1) or a corrupted receipt ("proof is invalid", exit 1).

**A1 — bound into the claim block.** `ClaimBlock` gains `zk_present` + `zk_image_id` +
`zk_receipt_sha256` + `zk_journal_sha256`. `derive_zk_binding` (compiler/src/evidence) derives them
structurally from the bundle's own risc0 sidecars, but only for a genuine receipt (non-placeholder,
non-dev, non-mock, `verify_status=passed`); otherwise `zk_present=false` — never fabricated. A
check/run bundle (no receipt) stays `zk_present=false` (PCA gate 13/13 still confirms this).

**A2 — verify re-derives, not re-trusts.** `verify_pca` re-derives the ZK binding from the bundle and
cross-checks (a tampered receipt hash / swapped ImageID / lying claim fail closed structurally). The
CLI (`verify_bundle_zk_receipt`, links risc0) additionally: ties the ImageID to the bundle's
`guest.elf` via `compute_image_id`, runs the real `risc0_zkvm::Receipt::verify` against it, and
confirms the receipt's journal matches the recorded digest. Corrupted receipt / wrong ImageID /
mismatched journal → exit nonzero.

**A3 — prove gate (`scripts/run_prove_gate.sh`, 11/11).** `tests/fixtures/zk_prove_bundle` is a
committed sealed receipt (from A0). The gate verifies it COLD in a fresh dir that never ran the prover:
claim binds the receipt; cold verify re-verifies the receipt against the ImageID; corrupted receipt +
swapped ImageID fail closed; a no-receipt bundle honestly reports `zk_present=false`. Re-run:
`bash scripts/run_prove_gate.sh`.

**Honest scope.** verify ties receipt → ImageID → `guest.elf` (hash-bound in the manifest) and
re-verifies cryptographically, but the `guest.elf` ← `source.anubis` compilation is TRUSTED —
recompiling to re-derive the ImageID needs the risc0 toolchain, which cold verify deliberately avoids.
`tier` is still `"checked"` (T2 tier grading is future work). Tests: +2 evidence (231 compiler), +2
CLI crypto (49 binary). Gates: prove 11/11, PCA 13/13, turing 13/13, fixtures 26/26. No regressions.

## Refinement-type foundation — B1 (non-erased static checking) + solver soundness (2026-07-10)

B1 generalizes the type checker to reject statically-known type incompatibilities, conservative by
design (zero false positives; dynamic expressions untouched). Dogfooded HARDEST: a 302-program
corpus/dogfood sweep + a 7-angle adversarial workflow (190 programs) that deliberately hunted false
positives and false negatives. Every finding was firsthand-reproduced before fixing. Gated waves:

- `f27fe2f`/`a50a6d9` **B1 core** — arithmetic/bitwise/unary/index checks; made literal-only after 2
  reassignment false positives; then `caec2e8`-era loop-var typing fix.
- `343bbc2` **solver soundness (i64)** — the check/solver modeled integers as 32-bit UNSIGNED but the
  runtime is i64 signed (verified: `u8 200+100=300`, `(-8)>>1=-4`), so it DISPROVED true assertions
  (`65536*65536 != 0`, `3e9+2e9 > 3e9`, `0-1 < 0`). Now 64-bit signed with signed comparisons; `/ %
  << >>` are non-modelable (div-by-zero / shift-mask mismatch → skipped, sound). Genuinely-false
  assertions still disproved. This unblocks B2–B4. **Superseded in the B2 wave below:** an early draft
  gave typed symbolic inputs / u32 params a `[0,2^w-1]` range — that was UNSOUND (a `u32` annotation is
  runtime-inert; `f(i64::MAX)` wraps), so the range was removed; a contract that needs bounds must
  state them via `requires`.
- `c9030a2` **type-coercion FPs** — i8/i16 now numeric; `+` inference returns string/list when either
  operand is (fixing `let s: string = 404+"x"` FP and `let n: u32 = 1+"a"` FN); reassigning an
  INFERRED binding is dynamic (only explicit annotations are held to their type; inferred types update
  flow-sensitively on reassignment).
- `c912464` **nested-constant + closure/block FNs** — widened the operand/index gate from bare-literal
  to constant-expression (`(2+3)[0]`, `("a"+"b")-1` now caught); `check_expr_semantics` now descends
  into `Lambda`/`Block` bodies (`|q| 5[0]`, `map([..],|x| 9[0])` now caught).
- `d066a8c` **cast-return** — `fn f() -> string { return 5 as u32 }` now rejected.

**Firsthand-verified**: every reject/accept above was observed via `anubis check`/`run`; the solver
disproofs were confirmed against z3 and the fixes re-verified. **0/302 false positives** after each
wave. **235 compiler tests; 49 binary; fixtures 26/26; PCA 13/13; prove 11/11; turing 13/13.**

**Honestly incomplete (deferred false negatives, checker stays SOUND — misses, never mis-rejects):**
enum/Option/Result-payload arithmetic (`match Some("hi") { Some(v) => v*2 }`), struct-field arithmetic
(`b.v - 1`), cast-to-inert-target laundering (`42 as string`), generic-instantiation over-erasure
(`Vec<u32>` param accepts a string), and `?` in a non-Result function. These need FieldAccess / enum
/ struct-field / generic type inference — structural typing that belongs to B4, not B1.

## Refinement-type foundation — B2 (first-class contracts: requires / ensures) (2026-07-10)

B2 adds `requires(P)` preconditions and `ensures(Q)` postconditions to functions, discharged by the
(now-sound i64) solver. A function's body + precondition must PROVE the postcondition at EVERY return;
callers ASSUME a callee's `ensures` and must SATISFY its `requires`. Dogfooded HARDEST: a 6-angle
adversarial workflow (overflow / ranges / multi-return / composition / body-mismatch / valid-rejected)
that hunted false proofs. It found **three real false proofs**, each firsthand-reproduced, fixed, and
locked with a regression test in the same wave:

| Category | Contract | Sig | Value | Sig |
|---|---|---|---|---|
| Contract discharge at tail return | under Command | `b2_contracts_verify_postconditions` | bounded `x+1>x` proved; `x-1>x` disproved | `cargo test -p anubis-compiler b2_contracts` |
| Multi-return: every path checked | under Command | `9381903` | early `return 0` vs `ensures(result>0)` disproved | in `b2_contracts_verify_postconditions` |
| Range-assumption soundness | under Command | this wave | unbounded `x+1>x` DISPROVED (was a false proof under the removed u32 range) | in `b2_contracts_verify_postconditions` |
| Composition guard (skipped precondition) | under Command | this wave | a callee's `ensures` is assumed ONLY when all its `requires` were checkable at the call site | `b2_contract_composition` |
| Fail-closed integer contract | under Command | this wave | an integer `ensures` over an unmodelable return is REJECTED, not skipped | in both B2 tests |

The three false proofs and their fixes:
1. **Multi-return** (`9381903`): only the tail return was verified, so `if x>5 { return 0 } return x+1`
   with `ensures(result>0)` passed while the early `return 0` violated it. Fix: discharge `ensures` at
   every return — tail under full body assumptions, each early/nested return under the precondition
   alone (a sound subset; can only mis-disprove a path, never mis-prove one).
2. **Range assumption**: u32/typed params were assumed in `[0,2^w-1]`, letting the solver "prove"
   `x+1 > x` even though `f(i64::MAX)` wraps to `i64::MIN` at runtime (annotations are inert). Fix:
   remove the range; params model as unbounded 64-bit. Overflow-vulnerable contracts are now correctly
   DISPROVED; a contract that needs bounds must state them via `requires`.
3. **Composition skipped-precondition**: at `let a = f(bad)` where `bad` is unmodelable, f's
   `requires` obligation was silently skipped yet f's `ensures` was still assumed — so `evil`, which
   returns `f(bad)`, "proved" its own `ensures` while returning a violating value. Fix (two parts):
   (a) assume a callee's `ensures` ONLY when EVERY `requires` was checkable at the call site; (b)
   fail-closed — an `ensures` whose returned value cannot be modeled is REJECTED
   (`ANUBIS_CONTRACT_UNPROVABLE`), never skipped.

## Refinement-type foundation — B2 soundness hardening (second adversarial sweep, 2026-07-10)

A second, harder adversarial sweep against the post-fix binary found **6 more independent false
proofs** — B2 was still unsound beyond the first three. Every one was firsthand-reproduced on the
release binary (`check` ACCEPT + a concrete runtime violation via `anubis run`) before any fix, then
closed and locked in `b2_soundness_fail_closed_regressions`. The unifying defect: several paths failed
**open** (silently accept) where they had to fail **closed**. The decisive realization: **contracts
are compile-time only — the transpiler emits NO runtime check for `requires`/`ensures`** (verified: a
violated `ensures(result == "wrong")` returning `"ok"` was accepted). So a *skipped* contract is
enforced nowhere; the old "non-integer contracts are left to runtime" claim was false. The fix makes
the named B2 regression class fail closed: in those fixtures an `ensures` is either discharged by the solver or the function is
rejected.

| Root cause | Firsthand violation (check ACCEPT → run) | Fix |
|---|---|---|
| A · truncating cast modeled as identity | `ident8(256)==256` "proved"; runs to `0` | `is_int_modelable`: a cast is modelable only if value-preserving (64-bit target); `x as u8/u16/u32` → non-modelable → fail closed |
| B · SMT-keyword / `bv*` param name dropped → z3 error → fail-open | `inc(model)` overflow contract accepted | mangle every SMT variable (`x`→`anb_x`) so it can't collide; a z3 parse error now fails **closed** |
| C · integer `ensures` over a non-modeled var vanishes | untyped `inc(x)` and reassigned-param contracts accepted-but-violated | `push_ensures_obligations` never skips: unmodelable concrete → REJECT |
| D · float param modeled as i64 bit-vector | `dbl(0.5)` "proves" `2x!=1`; runs to `1.0` | params modeled only when `is_integer_ty` (floats excluded) → float contract non-modelable → fail closed |
| E · integer literal > i64::MAX reduced mod 2^64 | `x + 2^64 <= x` "proved"; runs to a bigger f64 | `is_int_modelable` literal requires `parse::<i64>()` |
| F · self-contradictory assumptions → vacuous proof | `requires(x<100)` + `assume(x>1000)` "proves" `result>999999` | vacuity guard: a passing contract obligation whose assumptions are UNSAT fails closed |

Each row's verify command: `cargo test -p anubis-compiler b2_soundness_fail_closed_regressions`. The
binary-level battery (all REJECT / all valid-ACCEPT) is reproducible under `scratchpad/repro`.

**Firsthand-verified**: all 6 root-cause programs (+ the string-violation case) now REJECT; a *valid*
keyword-named contract (`inc(model)` with bounds) still PROVES — showing mangling fixed B in both
directions, not by blanket-rejecting. Valid bounded/composed/multi-return contracts still ACCEPT.
**238 compiler tests; 49 binary; fixtures 26/26; PCA 13/13; prove 11/11; turing 13/13; 41/41 example
corpus with 0 false positives.**

**Honest scope (what B2 does NOT prove — now fail-closed, not silently skipped):** `/ % << >>` in a
contract, string/list/bool-variable postconditions, floats, truncating casts, and any value the QF_BV
solver cannot model are all **rejected** (`ANUBIS_CONTRACT_UNPROVABLE`) — use a runtime `assert` in the
body for a dynamic check (that IS enforced at runtime). A tail-position direct call
`fn g()->u32 { helper(x) }` is rejected — bind via `let r = helper(x); return r;` to carry the
`ensures`. Loop-carried reasoning is B3. In the named B2 fixture set, a green `anubis check` records
that each emitted declared-contract obligation was discharged; this historical row does not prove
that every current AST position emits the obligation it should.

## Refinement-type foundation — B3 (loop invariants) + control-flow soundness (2026-07-10)

B3 adds `while ... invariant(P) { }` clauses verified by the Hoare rule (base case + inductive step +
frame + post-loop admit), readmitting loop-carried variables the solver otherwise drops. It was
dogfooded HARDEST: **FIFTEEN consecutive adversarial sweeps. Rounds 1-14 each found and closed a
distinct soundness defect; round 15 (70 programs, every combination of the 14 fixed areas, mandatory
assert flip-tests) found NO FALSE PROOFS — CONVERGED, firsthand-confirmed by a 15-case battery (10 hard
false-proof combinations all REJECT, 5 valid contracts/invariants all ACCEPT).** Two later defects
beyond the twelve tabulated below were also closed: (13) an `ensures` over a reassigned/shadowed
parameter proved against the mutated value while composition substitutes the entry argument — now
fail-closed (no `old()`); (14) `expr_to_smt_value` modeled a truncating cast as the identity, a false
`y == x` fact a loop invariant could force-model — now returns None for non-value-preserving casts.
Every defect was firsthand-reproduced (`check` accept + a concrete `anubis run` violation —
before fixing, and locked with a regression test in `b3_loop_invariants_verify_inductively`,
`loop_body_assert_not_discharged_against_stale_state`, `solver_modelability_is_function_local_and_shadow_safe`,
and the B2 contract tests). From round ~7 onward, the named inductive-invariant battery found no new
engine-local counterexample; later rounds surfaced pre-existing weaknesses in the general checker's control-flow/state handling that B3's
rigor exposed. The twelve closed defects:

| # | Class | Defect (all were `check`-accepted, runtime-violated) | Fix |
|---|---|---|---|
| 1 | Hoare | only the tail return verified (multi-return) | verify EVERY return path |
| 2 | Hoare | unsound `[0,2^w)` range on u32 params | removed; bounds must be stated |
| 3 | Hoare | composition assumed a callee's ensures when a `requires` was skipped | guard on all-requires-checkable |
| 4 | invariant | vacuous base case (contradictory pre-loop `assume`/`requires`) | vacuity guard on the base obligation |
| 5 | invariant | a nested `break`/`continue`/`return` escaped the loop | reject any escape at any depth |
| 6 | invariant | an auxiliary variable written in a branch/nested-loop frozen at a stale value | flat-body rule + drop every written var's frame fact |
| 7 | invariant | stale reassign before/between loops; loop-local `let` leak | drop+re-establish on reassign; scope body assumptions |
| 8 | invariant | a `let` shadowing a modeled variable reused its stale model | reject a shadowing loop-body `let` |
| 9 | general | an in-body `assert` discharged against the stale pre-loop value | HAVOC loop-written vars before the body |
| 10 | general | a write hidden in an `if`/`match`/block EXPRESSION missed by the write-scan | expression-aware `collect_assigned_roots` |
| 11 | general | conditional-path facts (zero-trip loop / untaken `if`) leaked as unconditional | scope + drop facts of conditionally-written vars |
| 12 | contract | a `return` in a `match`-arm / the implicit tail `if/else` value not checked against `ensures` | expression-aware return + tail-value scan |
| 12b | general | integer-modelability leaked across `let` shadowing and function boundaries | invalidate on shadow; reset per function |

**Firsthand-verified**: the demonstration (`ensures`/`assert` over a loop-carried variable, unprovable
without an invariant, provable with one) works; base-case/preservation failures reject; every one of
the twelve false proofs above now REJECTS; valid inductive invariants and bounded/composed contracts
still ACCEPT. **265 compiler tests; 56 tools tests; fixtures 26/26; PCA 13/13; prove 11/11; turing 13/13;
41/41 `.anub` example corpus with 0 false positives. (Reconciled 2026-07-11.)**

**Honest scope (what B3 does NOT do — all fail-closed, not silent):** invariants only on `while`
(for/loop rejected); the body must be a flat straight-line integer sequence (branches, nested loops,
`match`, escapes, shadowing lets, and expression-embedded writes are rejected — a real usability limit,
sound not silent); an accumulator invariant needs an explicit overflow bound; an in-body `assert` over
a loop-carried variable is deferred to the runtime (which enforces `assert`) rather than proved. A
call's `requires` in pure expression position (`g(bad)+1`) is not yet enforced — but no `ensures` is
assumed there either, so nothing is laundered in the cited fixtures (a completeness gap, not a false
proof). The B3 gate demonstrates discharge/rejection for its named supported cases; it is not evidence
that every current or future expression-holding position reaches that engine.

## A+ Maturity Gaps Closed (2026-07-11)

Reconciliation pass driven by the 2026-07-11 forensic dissertation. Every gap identified as material was
closed or explicitly marked DEFERRED with precise scope.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Unified gate suite (one command, all gates, stranger can run) | under Command | `scripts/audit_unified.sh` — 15 gates (G1-G15), JSON report | `bash scripts/audit_a_plus.sh --out out/a_plus_a15_frontdoor_20260724-154145` → 15/15 PASS, 0 FAIL, 0 SKIP | Friday, July 24, 2026 A15 re-seal. G14 **34/34** VZ guest; G15 8/8 `examples/feel/*` |
| Native CDCL Unsat RUP certificate (independent check) | under Command | `solver/src/lrat.rs`, `sat.rs` UnsatCert emission; lib returns Unsat only if `check_proof`; authoritative path also fragment-gated | `cargo test -p anubis-solver lrat sat::` (16+ lrat); `bash scripts/run_native_authoritative_gate.sh` | Fail-closed: missing/invalid cert → defer (`None`). Division still deferred. |
| Native SMT **default** flip (no env) | under Command | `compiler/src/middle/mod.rs` `native_authoritative()` default ON; opt-out `=0` | soak `out/native_default_flip_soak_20260725/`; decision `out/native_default_flip_seal_20260725/DECISION.md`; gate PASS | Product decision 2026-07-25. z3 still fail-closed cross-check when present. |
| A15 hostile full-language audit (current) | under Command | `implementer/a_plus_audit_run/20260724-154145/full_language_audit/A15_FULL_LANGUAGE_AUDIT.md` | re-run `bash scripts/audit_a_plus.sh` + compare gate_report | Documents F1–F4 (host fail-open marker, stale guest binary, clippy, stack) fixed and re-sealed |
| Offensive platform T1–T9 gate | under Command | `out/a15_offensive_t9_20260724-152746/report.json` + suite G14 | `bash scripts/run_offensive_platform_gate.sh` | **34/34** PASS, `isolation=tart-disposable-guest` |
| CI runs the real front door (not a weak subset) | under Command | `.github/workflows/ci.yml` execs `scripts/audit_a_plus.sh` on `macos-latest`; guarded by `ci_workflow_enforces_the_real_gate_suite_not_a_weak_subset` | `cargo test -p anubis-compiler ci_workflow_enforces` | Replaced the old 4-command subset (`cargo test` w/o `--all`, `clippy` w/o `--all-targets`, never ran G5-G15). Suite is self-contained on a stock runner: G10 is cold-VERIFY only (no risc0 prover / Metal), so CI is a public off-desk cold-verify witness. **Metal _proving_ in CI still NOT CLAIMED** (needs Apple-Silicon GPU); CI green not yet observed on GitHub from here — verified the runner locally (15/15) |
| Zero hard-coded author paths in Rust/TOML source | under Command | `grep -rn "sicarii/Desktop" --include="*.rs" --include="*.toml"` → 0 hits | `grep -rn "sicarii/Desktop" --include="*.rs" --include="*.toml" \| grep -v .claude/worktrees/` | All paths use `ANUBIS_RISC0_METAL_REFERENCE` env var |
| Cargo fmt + clippy + 643 tests green | under Command | `cargo fmt -- --check` + `cargo clippy --all-targets -- -D warnings` + `cargo test --all` | `bash scripts/audit_unified.sh` (G1-G3) | Thursday, July 23, 2026 rerun: `G3_test` reported `643 tests passed`. **Stale** — see the 649-test row above (2026-07-24) and the 2026-07-26 recount (707 compiler + 142 CLI); the test count has grown at every subsequent tranche and neither older figure should be cited as current. |
| PCA cold-verify tolerates tool-version drift (fixes a crown-jewel false-negative) | under Command | `evidence/mod.rs::claim_semantically_matches` + `verify_pca`; committed `tests/fixtures/zk_prove_bundle` (frozen by `anubis 0.2.0`) now cold-verifies under the current tool | `cargo test -p anubis-compiler verify_pca_tolerates_tool_version_drift verify_pca_cold_verifies_the_committed`; `bash scripts/run_prove_gate.sh` → 11/11 | Provenance `tool` version stays recorded + manifest/signature-protected but is excluded from the semantic re-derivation equality; every source/artifact-derived field + the ZK receipt crypto still match, so tamper (wrong receipt/ImageID/claim) still fails closed. Was silently failing G10 |
| Metal/ZK tests portable (skip when env not set) | under Command | 3 hybrid tests guarded by `ANUBIS_RISC0_METAL_REFERENCE` check | `cargo test --all` on a fresh clone without metal reference → 265 pass, 0 fail | Tests skip cleanly, not fail |
| Template Cargo.toml uses relative vendor path | under Command | `templates/Cargo.full.toml` → `vendor/risc0-circuit-rv32im` (was absolute) | `cat compiler/src/backends/native/hybrid/templates/Cargo.full.toml` | Emitted projects are portable |
| Env var unified to `ANUBIS_RISC0_METAL_REFERENCE` | under Command | `grep -rn "ANUBIS_METAL_HYBRID_PATH" --include="*.rs"` → 0 hits | same | evidence/mod.rs + emit.rs both use the canonical var |
| Documentation reconciled with HEAD | under Command | ARCHITECTURE_MAP.md, MATURITY_CLAIM_MATRIX.md, README.md updated | `diff` against prior | Test counts, script counts, example counts, hard-coded paths |
