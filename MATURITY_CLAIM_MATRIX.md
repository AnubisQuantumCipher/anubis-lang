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
| Turing-core gate (honest, no false-green) | REAL | `scripts/run_turing_core_fixtures.sh` compares stdout byte-for-byte to `.expected`; verdict derived, never default | `bash scripts/run_turing_core_fixtures.sh` → `Overall: PASS (11/11)` | Missing binary/expected/mismatch/nonzero-exit ⇒ FAIL |
| Arrays / lists (literal, index read/write, `len`, `push`, growable) | REAL | `tests/fixtures/turing_core/bubble_sort.anb` (→ sorted), `array_dp.anb` (→377/15) | `./target/release/anubis run tests/fixtures/turing_core/bubble_sort.anb` | `AnubisValue::List`; dynamic typing; enables real algorithms |
| `for v in a..b` range loops | REAL | `tests/fixtures/turing_core/for_range_sum.anb` (→5050) | `bash scripts/run_turing_core_fixtures.sh` | Desugars to counted while; bound evaluated once |
| Struct-literal-in-header ambiguity resolved (parser hang fix) | REAL | `header_position_is_not_a_struct_literal` unit test; `while running {}` / `for i in 0..n {}` | `cargo test -p anubis-compiler header_position` | Rust-style `no_struct` flag; also fixed latent `if flag {}` bug |

## Bounty-Grade PoC Kit (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Packing builtins (`p8`/`p16`/`p32`/`p64`, `cyclic`, list concat) | REAL | `examples/security/poc_packing_smoke.anb` → `16/65/65` | `./target/release/anubis run examples/security/poc_packing_smoke.anb --allow-research` | Requires `--allow-research` |
| Local process harness (`target_run`) | REAL | `examples/security/poc_local_overflow.anb` crashed=1 against `poc_kit/bin/vuln_local` | `bash poc_kit/build_vuln.sh` then `anubis run …/poc_local_overflow.anb --allow-research` | Local FS only; network URLs rejected |
| Gold local crash PoC | REAL | `poc_kit/vuln_local.c` + PoC fixture | `bash scripts/run_poc_kit_gate.sh` | Intentional lab oracle (abort if len>64) |
| Mutation process fuzz (real crashes) | REAL | `fuzz_report.json` engine=`mutation-process-v1`, unique_crashes≥1, distinct crash bins | `anubis fuzz --target poc_kit/bin/vuln_local --runs 50` | Mutator unit tests: multi-payload + len>64; not parse/typecheck |
| Security fixture runner needle honesty | REAL | EXPECT FAIL + ERROR_CONTAINS requires needle in log; wrong failure ≠ green | `bash scripts/run_security_fixtures.sh` + `security_fixture_matches` unit tests | Fixed inverted-needle false-green |
| Network targets forbidden | REAL | fuzz/target_run reject `://` | gate fixture `network_forbidden` | Fail-closed dual-use boundary |
| PoC kit gate | REAL | `out/poc_kit/report.json` / gate script | `bash scripts/run_poc_kit_gate.sh --out out/poc_kit` | 4/4 packing + crash PoC + fuzz + network deny |
| Full unscoped malware platform | **NOT CLAIMED** | docs/language/OFFENSIVE_PLATFORM.md | — | Engagement-scoped red-team platform only |

## Offensive Platform AOP T1–T7 (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Engagement scope fail-closed | REAL | engage-init/status | `anubis engage-init` | kill date + authorization |
| aop-2 AES-GCM encrypted beacons | REAL | live whoami result protocol aop-2 | gate `t1_encrypted_c2` | PSK in engagement |
| Agent keys + jitter | REAL | agent meta key_id + jitter_pct | `agent-generate` | sleep jitter in agent |
| mTLS cert material | REAL | `certs/server.crt.pem` | engage-init | ready; HTTP default |
| HTTP C2 + operator console | REAL | `GET /` HTML | gate `t7_console` | RBAC roles |
| DNS + UDS transports | REAL | listen log multi-transport | gate t3_* | lab DNS/UDS |
| LaunchAgent persistence | REAL | `persistence/*.plist` | `persist-launchagent` | install script included |
| Inject plan-only | REAL | PLAN_ONLY JSON | `inject-plan` | no silent inject |
| Lateral SSH scoped | REAL | external deny | `lateral-ssh` | allowed_lateral_hosts |
| ROP pattern/gadgets/browser | REAL | pattern-offset found | pattern-*/gadget-*/browser-harness | lab browser localhost only |
| XOR packer | REAL | packs/*.xor.pack | `pack-xor` | lab packer |
| Exploit modules + PoC kit | REAL | exploit success | exploit-run | crash oracle |
| Offensive gate | REAL | 16/16 | `bash scripts/run_offensive_platform_gate.sh` | T1–T7 |
| Full rustls mTLS handshake / live inject execute | PLANNED/PARTIAL | OFFENSIVE_PLATFORM.md | — | inject remains PLAN_ONLY by design |
| SMB/WinRM lateral **execution** | NOT CLAIMED | `lateral-smb` CLI | `anubis lateral-smb --host …` | **PLAN_ONLY**: structured plan, `executed=false`, no SMB sockets |
| RBAC queue + admin status | REAL | listener `/task` + `/admin/status` + `task-queue --operator` | gate `t7_rbac_queue` | `role_can_queue` / `role_can_admin` wired |
| Structured `allowed_targets` | REAL | engage-status + scope | gate `scope_targets` | Host/Cidr/LocalPath kinds |
| String scramble (lab) | REAL | packer + `string-scramble` | gate `t6_string_scramble` | XOR note helper, not crypto |
| ANBP proof-input blob magic | REAL | `proof_input.anbp` + metadata | prove --evidence | magic `0x414E4250`, header validated |
| Security fixture honesty contract in doctor | REAL | offensive-doctor JSON | gate `doctor_t17` | rejects false-green needle pattern |
| Agent standalone cargo project | REAL | `[workspace]` empty in agent Cargo.toml | gate `t1_agent_encrypt` | no parent-workspace collision |

## RISC0 parameterized proofs (2026-07-09)

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Program-bound guest (input-free) | REAL | factorial journal 120, fib 55, distinct ImageIDs | `bash scripts/run_proof_binding_gate.sh` | commit 164488a |
| `proof_input_u32` / guest `env::read` map | REAL | guest contains `anubis_load_proof_inputs` + lookup | prove `examples/proof/proof_factorial_input.anb` | ABI v1 |
| CLI `--input-json` / `--input-file` | REAL | input_sha256 in metadata | prove with `--input-json '{"n":5}'` | exclusive flags |
| Same program, different inputs → different journals | REAL | n=5→120, n=6→720 | out/proof_factorial_5 + _6 | same ImageID |
| Same program → same ImageID | REAL | ImageIDs equal across n=5/n=6 | metadata compare | program-bound |
| input_sha256 + parameterized metadata | REAL | risc0_metadata.json schema 1.3 | prove --evidence | canonical JSON hash |
| Receipt verify for parameterized | REAL | verify_status=passed, !dev_mode | both n=5 and n=6 | Metal ref required |
| Parameterized proof gate | REAL | scripts/run_parameterized_proof_gate.sh | `bash scripts/run_parameterized_proof_gate.sh` | opt-in ~1–2 min |

## RISC0 Proof Bound to the Program (2026-07-09)

Retires the biggest honesty debt: `prove --backend risc0` previously proved a HARDCODED `x*6`
circuit on input `77`, decoupled from the input `.anb`. It now compiles the actual Anubis program
into the guest, so the ImageID (derived from that guest ELF) binds the receipt to the program.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| RISC0 guest compiled from the Anubis program (not a fixed circuit) | REAL | `out/proof_factorial/backend/risc0/guest/src/main.rs` contains `anb_factorial`/`anb_main`/`env::commit`; `risc0_metadata.json` `guest_binding=anubis-program` | `bash scripts/run_proof_binding_gate.sh` | `lower_program_to_guest` reuses the Turing-complete lowering; `std` guest |
| Receipt proves the program's real result (journal = computed value) | REAL | `proof_factorial` journal `120` = factorial(5); `proof_fib` journal `55` = fib(10); both `verify_status=passed`, `dev_mode=false`, `mock_prover=false` | `bash scripts/run_proof_binding_gate.sh` → `Overall: PASS` | journal decoded u32 LE; verified via `Receipt::verify(image_id)` |
| Proof is program-bound (different program → different ImageID) | REAL | factorial ImageID `2358913413…` ≠ fib ImageID `4137336513…` | `bash scripts/run_proof_binding_gate.sh` (distinct-ImageID check) | ImageID = cryptographic commitment to the compiled program |
| Real derived ImageID + real `Receipt::verify` + strict non-dev | REAL | `risc0_metadata.json` (real u32x8 ImageID, `image_id_is_placeholder=false`); `verify-receipt` re-extracts journal | `anubis verify-receipt --receipt … --image-id …` | bound to vendored patched `risc0-circuit-rv32im` at the reference path |

## Gate 11 Metal CPU vs Metal-hybrid parity (2026-07-09 honesty re-seal)

Retires same-dir / sealer-`|| true` / trivial-guest debt. Fixtures are program-derived (return `42` /
`x*6`); CPU and Metal prove into **distinct** `*_cpu` / `*_metal` dirs; sealer requires
`paths_distinct` and is fail-closed under `--require-metal`.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| Distinct per-lane out dirs (no same-path compare) | REAL | sealer `parity.paths_distinct=true` on all 3 fixtures; script path check | `bash scripts/check_metal_parity.sh --require-metal --out out/a_plus_gate11_parity_continue` | A15: `implementer/a_plus_audit_run/20260709-095128/gate11_metal_parity` |
| CPU lane observed = `cpu` | REAL | all fixtures `cpu.lane_observed=cpu` + `R0_DISABLE_METAL=1` | same | not inferred from host only |
| Metal-hybrid lane observed = `metal-hybrid` | REAL (local Tier-2) | all fixtures `metal.lane_observed=metal-hybrid` | same | host aarch64/macos; CI Metal **NOT CLAIMED** |
| Journals match (program commit = 42 LE) | REAL | all 6 `journal.bin` = `2a000000`; sha256 `e8a4b2ee…d7cc` | same | extracted journals, not hardcoded |
| ImageID match per fixture (both lanes) | REAL | `image_id_match=true` per fixture | same | same guest ELF per program |
| Different programs → different ImageIDs | REAL | hello ≠ arithmetic ≠ symbolic_safe ImageIDs | jq fixtures | program-bound guests post Gate 10 binding |
| Both receipts verify | REAL | `receipt_verify=passed` both lanes | same | real `Receipt::verify` |
| Sealer fail-closed + A15 no `\|\| true` | REAL | `seal_rc=0`, `overall_verdict=PASS`; `gate11_a15_reproduce.sh` exits nonzero on fail | `./target/release/anubis gate11-metal-parity … --require-metal` | sealer exit no longer ignored |
| Overall Gate 11 under `--require-metal` | REAL (local Apple Silicon) | `overall_verdict=PASS` | full checker + sealer | third-party / hosted CI Metal still **NOT CLAIMED** |

## Multi-field journals (2026-07-09)

Public outputs beyond a single u32: `return [a, b, …]` commits each field via
`anubis_commit_journal` (scalar path remains v1-compatible).

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| List return → multi-u32 journal | REAL | a=3,b=4 → journal `[7,12]` (8 bytes) | `bash scripts/run_multi_field_journal_gate.sh` | `proof_multi_field.anb` |
| Different multi-field inputs → different journals, same ImageID | REAL | a,b=(3,4) vs (5,6) → `[7,12]` vs `[11,30]` | same gate | program-bound |
| Scalar journal still 4-byte u32 | REAL | factorial n=5 → 120 | same gate regression | no break of parameterized path |
| Private witness / redacted input split | NOT CLAIMED | inputs already not in journal (env::read only) | — | no selective redaction metadata yet |
| Named journal fields (`proof_commit_u32`) | REAL | `journal_fields` + `journal_decoded.json` | `bash scripts/run_named_journal_gate.sh` | names from guest source; values from journal |
| Host journal decode (u32 LE sequence) | REAL | `decode_journal_u32s` / `journal_fields_json` | same gate | synthetic `field_N` if unnamed |
| `proof_assert` fail-closed in guest | REAL | out-of-range fails prove/run | `examples/proof/proof_assert_range.anb` | private x not in journal names |
| `proof_commit_bool` | REAL | ok=1 in journal_fields | power gate | 0/1 public bit |
| Engagement action receipt chain | REAL | `evidence/receipts/chain.jsonl` + tip | `anubis receipt-verify` | hash-chained; tamper fail-closed |
| Power gate (language+proof+receipts) | REAL | `scripts/run_power_gate.sh` | bash gate | compound capability seal |
| Enums (`enum` + unit/tuple variants) | REAL | `examples/enum_status.anb` | `anubis run` → 42 | `Status::Err(42)` |
| `match` expressions + bindings | REAL | same + `proof_enum_status.anb` | `bash scripts/run_enum_match_gate.sh` | `_` wildcard supported |
| Struct-like enum variants (`Err { code }`) | REAL | `examples/enum_struct_variant.anb` → 99 | `bash scripts/run_lang_trio_gate.sh` | match named bindings |
| Maps / dictionaries `{k:v}` | REAL | `examples/map_dict.anb` → 6 | same gate | index get/set, for-in keys |
| if-expressions `let x = if c {a} else {b}` | REAL | `examples/if_expr.anb` → 7 | same gate | else required; else-if chains |
| Lang power trio (maps+struct-enum+if-expr) | REAL | `examples/lang_power_trio.anb` → 42 | same gate | combined executable surface |
| Prove if-expr + struct-enum + named journal | REAL | `examples/proof/proof_lang_trio.anb` | same gate (when metal ref present) | secret private; code+ok public |
| A+ call-site + let type checks | REAL | `a_plus_rejects_bool_for_u32_param` | `cargo test -p anubis-compiler a_plus_rejects` | `ANUBIS_TYPE_MISMATCH` / arity |
| A+ match exhaustiveness | REAL | `a_plus_match_non_exhaustive_fails_closed` | `cargo test -p anubis-compiler a_plus_match` | missing arms fail check; `_` OK |
| Hex/bin/oct integer literals | REAL | packing smoke uses `0x41414141` | `anubis run …/poc_packing_smoke.anb --allow-research` | lexer → decimal token |
| PoC `target_run` named TargetRun | REAL | `r.crashed` / `r.signal` … | `poc_local_overflow.anb` + list-compat `r[0]` | struct fields + index order |
| `for x in collection` list iteration | REAL | `examples/for_in_list.anb` → 60 | `bash scripts/run_for_in_gate.sh` | also turing fixture sum 15 |
| `for i in a..b` range (regression) | REAL | for_range_sum → 5050 | same gate | half-open |
| Prove for-in sum of private inputs | REAL | proof_for_in_sum journal 60 | same gate | a+b+c with proof_assert |

## Backend Unification Keystone (2026-07-10)

Retires the biggest structural debt: `build`/`prove` used a template/pattern-matched native emitter
(`backends/native/mod.rs::lower_to_native`) that faked execution (research template printing
`poc_memory_op_executed`; a `safe_execution` stub that never ran the program), while `run` used the
faithful whole-program transpiler. Now **every command shares `backends::run::lower_program_to_rust`** —
evidence, proof, and execution lower the *same* program.

| Claim | Status | Evidence | Command | Notes |
|-------|--------|----------|---------|-------|
| `build`/`prove` native artifact uses the faithful lowering (runs the real program, not a stub) | REAL | `build_of_program_with_main_emits_faithful_runnable_artifact` (lib.rs); CLI: `keystone-real-exec`/`42`, 0 `safe_execution` in emitted `.rs` | `cargo test -p anubis-compiler build_of_program_with_main` ; `anubis build FILE.anb --out d && d/anubis_out` | non-hybrid programs with `fn main` route through `lower_program_to_rust` |
| `run` vs `build` output parity (same program → same result) | REAL | run + build-artifact byte-identical on closures/map/reduce/for-in program | `anubis run p.anb` vs `anubis build p.anb && ./anubis_out` → `diff` identical | both pin `rustc --edition 2021` (main.rs:2829, native/mod.rs:40) |
| Non-runnable program (no `fn main`) → honest analysis-only marker (no fabrication) | REAL | `mainless_research_snippet_lowers_to_honest_analysis_marker`; CLI marker reports real taint `x: tainted<u32>`, `constraints: 4`, reason `no fn main()`, 0 `poc_memory_op_executed` | `anubis build examples/research_poc.anubis --out d && d/anubis_out` | reports mode/taint/constraints/reason; substance in evidence bundle |
| Brittle "research lowering requires assume(...)" gate RETIRED (honesty debt 0.3) | REAL | `research_snippet_without_assume_lowers_via_faithful_path_gate_retired` | `cargo test -p anubis-compiler research_snippet_without_assume` | now emits honest marker instead of a template gate error |
| Safe-mode enforcement preserved (no runnable artifact for violations) | REAL | raw ptr → `ANUBIS_RAW_POINTER_IN_SAFE`; tainted sink → `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY`; both exit=1, no artifact | `anubis build <safe-with-rawptr>.anb` / `<safe-tainted-sink>.anb` | enforcement is in `typecheck`, upstream of lowering |
| Evidence bundles still validate after unification | REAL | safe + research bundles both `bundle valid: true` | `anubis build FILE --evidence --out d && anubis verify d/evidence-*` | tamper-evident hash chain intact |
| Hybrid programs unchanged (RISC0+Metal emitter) | REAL | `hybrid_host_compiles_and_dispatches`, `parses_hybrid_and_spec_blocks` green | `cargo test -p anubis-compiler hybrid` | `hybrid { }` still routes to `lower_hybrid`; known limit: only detected in the first fn (pre-existing) |
| Independent adversarial review (4 lenses, findings verified) | REAL | 1 low finding (edition mismatch) found+fixed; 2 dismissed as non-regressions | workflow `keystone-adversarial-review` | enforcement lens also proven firsthand via CLI |

## Language Correctness Sweep — Wave 1 (2026-07-10)

A 13-cluster adversarial cross-feature sweep (each finding verified firsthand) surfaced 32 confirmed
defects. Wave 1 fixed 13 (all high-severity + the number/crash classes), each with a pinned
regression test. Suite: **204 compiler tests, 0 failed** (`cargo test -p anubis-compiler`).

| Fix | Status | Test / evidence | Was |
|-----|--------|-----------------|-----|
| `as` cast binds tighter than binary ops (`300 as u8 + 1` = 45) | REAL | `cast_binds_tighter_than_binary_ops` | cast silently voided, `+1` dropped → 300 |
| Struct `==` is field-order-independent | REAL | `struct_equality_is_field_order_independent` | positional zip → `{x,y}` ≠ `{y,x}` |
| Named functions bind by bare name in `let` | REAL | `named_functions_bind_by_name_in_let` | `let f = double` → ANUBIS_UNKNOWN_VARIABLE |
| Compound assign evaluates a side-effecting index once | REAL | `compound_assign_evaluates_index_once` | `xs[pop(sel)] += 5` popped twice |
| Signed narrowing cast sign-extends (`255 as i8` = -1) | REAL | `integer_casts_and_wide_literals` | masked unsigned → 255 |
| Full-width radix literals (`0xFFFFFFFFFFFFFFFF` = -1) | REAL | `integer_casts_and_wide_literals` | i64 parse failed → 0 |
| `i64::MIN` decimal literal is exact | REAL | `integer_casts_and_wide_literals` | coerced to f64 |
| Named function passed to `map` pads (no panic) | REAL | `named_function_arity_pads_not_panics` | index-out-of-bounds panic |
| `assert`/`assume` work in expression position | REAL | `assert_and_assume_work_in_expression_position` | `Expr::Other` → unsupported-expr error |
| Empty `${}` interpolation is a clean diagnostic; empty `""` lowers | REAL | `empty_interpolation_and_empty_string_are_handled` | crash / `parts.remove(0)` panic |
| Built-in `Some/None/Ok/Err` render bare; maps show quoted keys | REAL | `display_forms_option_result_map_and_user_enum` | `Option::Some(x)`, `{a: 1}` |
| Actionable "unsupported expression" errors (no `Discriminant(N)`) | REAL | error arm split in `safe_run_expr` | opaque `Discriminant(28)` |

**Remaining (19, triaged for later waves):** map key coercion (int index of string key; int/float key identity);
mutating builtins on a struct-field place; duplicate struct-field literal accepted; method/closure arity not
enforced (a design call — currently pads with 0); struct display in declaration vs insertion order; multi-arg
generics (`Map<int,string>`) in annotations; or-pattern-with-wildcard exhaustiveness; `?` on a non-Option/Result;
unknown-var not flagged in an `if` condition; plus a tail of low-severity edge cases. Fixture-harness debt:
`run_language_fixtures.sh` is state-sensitive (stale `out/` false-fails) and `missing_semicolon.anb` is stale
(semicolons are optional now).
