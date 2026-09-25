# Mission defect inventory (from the Stage-A subsystem review)

Derived from `docs/mission/review/01-04`. Each row is a defect or gap found by reading the source;
"verified" means confirmed by a probe or by an exact code citation, "reported" means asserted by the
mapper and not yet independently reproduced by the integrator. This inventory feeds `docs/CLAIMS.md`
(soundness defects) and the roadmap; it is not a second status authority.

Severity: S1 unsound/false-claim in a shipped assurance; S2 gate/evidence that can pass without asking
its question; S3 precision/over-rejection or crash on input; S4 correctness/quality; S5 docs/hygiene.

## Assurance chain (03)

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| A-GATE-1 | S2 | G18 native_authoritative compares default mode against `ANUBIS_NATIVE_AUTHORITATIVE=1`, but the default has *been* that mode since 2026-07-25, so "0 mismatches" is guaranteed | review 03 §CI/gates | reported |
| A-GATE-2 | S2 | G18 drift check greps `PROVEN_OP_TAGS`, a string list no Rust code reads; admission is decided by `fragment.rs` `term_ok`/`pred_ok` | review 03 | reported |
| A-GATE-3 | S2 | `run_native_shadow_gate.sh` (cited by PROOF_CORRESPONDENCE as link-3 evidence) is in no roster and its floor file is absent | review 03 | reported |
| A-GATE-4 | S2 | No gate runs `verify_proof_bundle.py` or drat-trim on a produced bundle; `evidence-verify` replays with in-repo Rust and reports PASS even when no obligation has a certificate | review 03 | reported |
| A-EVID-1 | S1 | `solver_replay.json` covers only the first obligation yet labels it `counterexample_replayed / replay_valid:true` even when it passed | evidence/mod.rs:532-551 | reported |
| A-EVID-2 | S4 | PCA hard-codes `solver_backend:"z3"` though native is the default authority | review 03 | reported |
| A-EVID-3 | S1 | a `wrap-safety:` obligation z3 returns UNKNOWN on stays UNKNOWN; `check`+PCA treat it as pass while the bundle's own solver check treats it as FAIL | review 03 | reported |
| A-EVID-4 | S4 | obligation identity = kind prefix + raw SMT text, no source location; proof files keyed by position; no cross-check of proofs.json vs solver.json | review 03 | reported |
| A-LEAN-1 | S1/S5 | only `SecurityLabel` is byte-for-byte linked to Rust (G31); `BitBlast` linked by theorem-name grep; 14 modules checked only for compiling; 5 security/effect models fully disconnected from the compiler | review 03 | reported |
| A-SOLVER-1 | S1 | when native declines, z3 `unsat` is accepted with no certificate (REG-002) | review 03 | reported |
| A-CI-1 | S2 | one workflow, one macOS job; no Linux CI; VM/Metal lanes operator-run outside CI | .github/workflows | reported |

## Frontend / language (02)

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| F-PARSE-1 | S1 | malformed source (`let x = );`, `let x = 1 + ;`, unexpected tokens in `requires`/`assert`, `"${1+}"`) gets `check` rc 0 and JSON `verdict:"pass"`; the parser makes a placeholder node without an error (frontend/mod.rs:4042) | probes, review 02 | **fixed** (parser commit after b7953650): unexpected token and mid-expression EOF are diagnostics; matrix `parse_*` |
| F-PARSE-2 | S3 | no parser recursion-depth limit; deeply nested input stack-overflows the compiler (exit 134) | probes, review 02 | **fixed** (same commit): `MAX_PARSE_DEPTH`=256 guard on primary/statement/pattern/interpolation paths; 200k-deep input is a diagnostic, rc 1, not SIGABRT |
| F-SPAN-1 | S3 | most AST nodes carry no source span; only `let` among statements does; solver/semantic diagnostics always `location:None` | review 02 | reported |
| F-RESOLVE-1 | S1? | only functions are renamed per module; structs/enums/impls share one namespace; `a.b` and `a_b` collide; same-last-segment imports overwrite | review 02 | reported |
| F-JSON-1 | S1 | F-PARSE-1 violates the DIAGNOSTICS_JSON rule that verdict is `fail` whenever the check failed | review 02 | reported |
| F-PKG-1 | S3 | git fetch passes URL/rev without `--` separator (resolve_deps.rs:611); a `-`-leading value is read as a git option; predictable shared temp dir; `http://` registries accepted | review 02 | reported |
| F-PKG-2 | S4 | lockfile `version` never validated; stale-lock check confirms only dependency name, not version/source | review 02 | reported |

## Roadmap / self-host (04)

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| R-DOC-1 | S5 | certificate residual: ROADMAP.md:116-117 says CLOSED (LRAT); ROADMAP_AI_ERA:252 + CLAIMS item 6 say still DRAT | review 04 | reported |
| R-DOC-2 | S5 | self-host state given as DONE / PARTIAL / sealed / unsealed across four docs | review 04 | reported |
| R-DOC-3 | S5 | CLAIMS row 9 says closed (:1572) and open (:1574); row 3 fix (13373e31) not recorded; receipt says 13 fixtures, 12 exist | review 04 | verified (12 fixtures on disk) |
| R-SELFHOST-1 | S4 | self-host is one file; top-level parses only `fn`/`enum`; cannot parse struct/impl/trait/import/generics/float/`as`/`loop`/`?`; no contract engine; gates compare per-file on hand-picked fixtures, none in CI | review 04 | reported |
| R-NUM-1 | S5 | stamped pass-counts hand-typed though README says never; gate roster quoted 29/30/31; AGENTS says main is d8742aab | review 04 | verified (roster is 31) |

## Already being fixed this session

| id | sev | defect | fix |
|---|---|---|---|
| H-ETXTBSY | S4 | four test spawn sites bypass `retry_while_exec_busy`, so a fork/exec ETXTBSY race fails the suite under parallel load (three different tests across three runs) | **fixed** 76de6d3a (848/0 at 8 threads, 0 ETXTBSY) |
| M-BUILTIN-HOF | S1 | `map([1,-2], f)` with contracted `f` checks clean (direct-lane builtin HOF emits no obligation); the valid `apply(f,positive)` twin is refused | **fixed** 7465aa46 for apply/call and element-wise HOFs over literal collections; unknown collections and reduce/compose/fold remain open (M-HOF-UNKNOWN) |

## New findings this session

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| M-HOF-UNKNOWN | S1 | a contracted function mapped over a NON-literal collection (`map(xs, f)`), and any function passed to `reduce`/`compose`/`fold`, is still not discharged | 7465aa46 scope note | open |
| N-LOWER-1 | S3 | array literals nested deeper than ~128 fail native lowering: the generated crate hits rustc's default macro recursion limit expanding nested `vec!` (clean `ANUBIS_UNSUPPORTED_NATIVE_LOWERING`, check passes). Pre-existing; depth 128 runs | probe, same result on pre-change pin | open |
| L-LINT-1 | S2 | the workspace clippy gate (`-D warnings`) fails on Linux: 7 errors (macOS-only imports/methods unused off macOS, a needless `return` in a non-macOS cfg block, two newer-clippy lints). Invisible because CI is macOS-only (A-CI-1) | cargo clippy on aarch64 Linux, rustc 1.98.1 | fixed in the Linux-lint commit (macOS side not compiled locally; macOS CI is the witness) |
| L-RUN-2X | S4 | `anubis run` off macOS typechecked the whole program, discarded the result, then `run_anubis_source` typechecked it again: every Linux run did the full analysis twice | tools/anubis/src/main.rs run_anubis_source_signed | fixed in the Linux-lint commit |
| D9 | S1 | unannotated formal: declared field qualifier not applied (runtime-confirmed secret print) | fc645f8f | **fixed** for provable call-site types; residual `d9_unknown_arg_type` open |
| T-SINKARG | S1 | closure stored by a field/index write dropped from `field_closures`; `b.f = g; b.f(input())` reached `shell`/`print` unchecked (7 shapes) | 2047df10 | **fixed** |
| E-POISON-1 | S2 | three PCA honesty tests required a live false accept to stay open | 2047df10 | **fixed**: accepted-program fixture + guard test |
| C-CALLPOS-1 | S3 | carrier lane resolved call-position names scope-first; runtime resolves user functions first -> over-rejection with an unreachable counterexample (c41) | 90b70ff1 | **fixed**; s12/c41 recategorized with runtime witnesses |
| L-SHADOW-1 | S3 | language: a local/formal does NOT shadow a same-named user function in call position (it does in value position). `let check = strict; check(x)` silently calls a global `check`. Unspecified in SPEC | runtime witnesses in recategorizations.tsv | open — language decision (diagnostic now, lexical scoping via an edition) |
| L-REPL-1 | S4 | REPL interpreter resolves builtins (`print`, `len`) before user functions and has no closures; native resolves user functions first | compiler/src/interp/mod.rs | open |
| M-DIRECT-REQ (reproduced) | S1 | `fn g(x: i64) { f(x) }` with `f` requiring `x > 0`, no requires on `g`: `g(-1)` checks clean (g's params are unmodeled, the obligation is never emitted). Untyped variant (s02) is the unmodelable-clause drop | s01/s03, runtime | **fixed** d90082d0: s01/s03/s11 DISPROVED, s02/s04 UNDECIDED (`requires-unresolved@`) |
| ENV-TOOLCHAIN | S5 | since 08:55 2026-09-23 rustup is installed on this host (not by this session): `cargo fmt`/`cargo clippy` resolve to the pinned nightly-2026-05-10 (CI's), builds still use Arch rustc 1.98.1. Pins before and after differ in lint/format tooling, not in the build compiler | ~/.cargo/bin/rustup mtime | recorded |
| EXT-VERIFY-1 | — | independent bubblewrap verification of the parser fix (another session): old pin aborts, fixed binary diagnoses, 20/20 checks; binary ~/Work/anubis-crash-fix/anubis-fixed sha256 09b8a172…, source between d5ed1d72 and a94537f7, build flags unrecorded — corroboration, not a source-bound pin | ~/Work/anubis-crash-fix/verification.jsonl | recorded |
| P-FMT-1 | S5 | 76de6d3a/e99db1d1/7465aa46 were committed without `cargo fmt`; the fmt gate failed from 76de6d3a until b7953650 | cargo fmt --check at c87ad1cf rc 0, at 7465aa46 rc 1 | fixed b7953650 |

## Notes on trust

Every "reported" S1/S2 row must be independently reproduced by the integrator before it is acted on as
fact, and before any claim depending on it is changed. The mappers ran against a binary ~2.7h older than
HEAD; probe-verified frontend findings must be reconfirmed on the current source.

## Analysis layer (01) — code-cited

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| M-DIRECT-REQ | S1 | an unmodelable `requires` on a DIRECT or method call is dropped with no obligation and no diagnostic: `discharge_call_requires` -> `carrier_unresolved_clause` returns early when `ctx.carrier_origin` is None (mod.rs:8340-8343), which holds for every non-carrier call; callee still assumes the requires, contracts not runtime-enforced (mod.rs:20082). Unmodelable `ensures` is refused, so this is asymmetric | mod.rs:8340,20082 | **fixed** d90082d0 (see the reproduced row) |
| M-MATCH-TRUNC | S1 | statement-level `match`/`if let` arms run `ctx.solver_obligations.truncate(obl_mark)` (mod.rs:10673/10719/10771/10801), discarding direct `requires@` and carrier `requires-unresolved@` obligations, not just arm-body asserts | mod.rs:10673+ | **fixed** e99db1d1 (reproduced: rc-0 silent accept pre-fix) |
| M-IFLET-EXPR | S1 | in `discharge_calls_in_expr` the `if let` then-branch and lambda bodies are never discharged (mod.rs:9084-9092); a value block re-binding a tracked name is skipped (mod.rs:8926) | mod.rs:9084,8926 | statement if-let arm **fixed** e99db1d1; expression-position if-let/lambda/value-block parts still to reproduce |
| M-GLOBAL-SCOPE | S1 | global name resolved before local scope in `fn_identities_of_d` (740), `fn_alias_of_d` (484), `closure_arity_of` (440), `carrier_mentions_function` (8270); only `carrier_identities` looks at scope first | mod.rs cited | reported |
| M-SINK-ARGS | S1 | a sink builtin stored in a field/container and called `b.f(p, input())` is capability-charged but its arguments never get the taint->sink check (mod.rs:1926-1928, 14212-14240) | mod.rs cited | candidate |
| M-JOIN-CLOSURE | S1 | branch joins don't merge `closure_lambda`/`field_closures` (mod.rs:7291-7397), so after an `if` a var may still hold the pre-branch closure | mod.rs:7291 | candidate |
| M-CTX-LEAK | S3 | `annotated_vars`/`known_bindings` never cleared between functions -> possible over-rejection and an unknown-var-check gap | review 01 | candidate |
| M-STRINGTY | S4 | `ty::Ty` exists but the checker never uses it; types are `Option<String>`; `carrier.rs::carrier_class` has no consumer outside its tests | review 01 | reported |

Architecture note: `SemanticContext` has ~70 name-keyed fields; a single binding keeps four separate
"which function is this" records, and trifecta + contract_carrier each keep their own alias map. This
is the string/name-reasoning surface Stage D must replace with binding-ids, place paths, and typed
callable summaries.

## Path precision (d90082d0, 2026-09-23)

The change that fixes M-DIRECT-REQ makes contract, assert and wrap checking path-sensitive (SPEC
"Contract checking: paths and refusals"). It went through three independent adversarial review
rounds, and every reproducer is a matrix case (`pp_*`, `rv_*`, `rv2_*`, `rv3_*`).

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| P-F64-LIT | S1 | an int literal passed into an `f64` parameter was encoded as a bitvector: the float obligation was ill-sorted, and z3's sort error was filed as a refutation (a false proof) | `rv_f64_param_int_literal_arg` | **fixed** d90082d0 |
| P-FLOAT-LEAK | S1 | facts about a float `let` inside a branch or loop leaked past its scope and proved obligations after it | `rv_branch_local_let_float_leak`, `rv2_loop_local_let_float_leak`, `rv2_for_local_let_float_leak` | **fixed** d90082d0 |
| P-WRAP-LATER | S1 | wrap safety was checked under the end-of-body state, so a later assignment, loop post-state or in-body invariant could justify an earlier operation that wraps | `rv3_wrap_later_assignment`, `rv3_wrap_endstate_loop`, `rv3_wrap_inbody_invariant_poststate`, `rv3_wrap_checked_at_statement` | **fixed** d90082d0 |
| P-EX-WRAP | S3 | shipped example `hall_of_two_truths` failed `ANUBIS_WRAP_RISK` in every pin back to 7220c4b1 (`i + 1` with `i` in `0..len(xs)`) | pin bisection | **fixed** d90082d0 (range variable modeling) |
| P-EX-NARROW | S4 | six shipped examples (release_gate, core_language_showcase, ennead, nexus, anvil, seshat) relied on unchecked facts | corpus diff | **repaired** with checked narrowing; runtime output byte-identical |
| P-SORT-1 | S1 | the solver pipeline accepts an ill-sorted query as a proof: z3 reports a sort error (e.g. `bvsgt` on a FloatingPoint term) and the obligation is still filed `rup_refutation`/PASS. P-F64-LIT removed the known producer; no sort check guards the next one | review round 1 | **fixed** fb57f472: the native parser sort-checks every assertion before lowering and declines an ill-sorted or logic-violating query; a native verdict on a query z3 rejects fails closed at every cross-check; evidence publishes `rup_refutation` only for accepted obligations |
| P-PREC-1 | S3 | valid programs refused UNDECIDED: loop results and loop-carried values, list-iteration variables, guards over havoced values, closures copied in loops, struct fields after loops, values flowing out through a branch-local, range loops with early exits (`?`, may-exit user functions, `break`) or bounds written or shadowed in the body | matrix pathprec-final: 20 `rv*` wrong-class cases + c17 | **open**: refusals, not silent accepts |
| P-DIVERGE-1 | S3 | a call to a user function that always panics does not end the caller's path (review finding f5) | SPEC | **open** |
| P-JOIN-FIELD | S1? | field symbols (`1fld_<root>_<f>_e`) are missing from the join's exclusion sets. This is masked today because field modeling does not survive a join (review round-2 probes fa01–fa05 fail closed UNDECIDED; not matrix cases). It becomes live the moment field facts are joined | review round 2 | **open** (latent) |
| P-OVERAPPROX-NAME | S4 | over-approximation labels are keyed by obligation name, so identically named obligations share a label; this errs only toward UNDECIDED | review round 2 | open |
| P-Z3-LOAD | S3 | a z3-trusted float obligation flips with host load: `float_contract_monotonicity_accepts` failed once on the pre-change pin and passes on rerun (see CLAIMS "float contract lane is NON-DETERMINISTIC") | corpus diff | open; reproduced again 2026-09-23 as `phase3_qf_fp_float_contract_lane` failing once under memory exhaustion and passing alone in 9 s |
| P-C53 | S3 | c53 (valid requires-seeded recursion through an untyped parameter) moved from DISPROVED to MIXED: the recursive call's precondition is now refused explicitly instead of dropped, while the pre-existing carrier false counterexample on `f` remains | matrix pathprec-final | open (carrier) |
| P-CERT-COUNT | S4 | c53's output says "certificates: 2/2 obligations discharged by a machine-checked refutation" while one of the two failures was never sent to a solver (`requires-unresolved@`). The same line appears on the pre-change pin | c53 output | **not a defect** (verified): the line counts DISCHARGED obligations only; c53 has 2 PASS obligations, both certified, and its 2 failures are reported by the refusal path |

Still open from before this change and unchanged by it: the shared silent accepts c43, c64, c65 and
`d9_unknown_arg_type` (closed later by 1b40f653), and the s09/s10 shadow chains (L-SHADOW-1).

## Native solver and float lane (fb57f472, 2026-09-23)

Found while fixing P-SORT-1, by three independent review rounds and a four-lens adversarial workflow
(58 agents, three refuters per finding). Every confirmed item has a test or matrix case.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| N-NAN-EQ | S1 | `=` over Float64 was lowered as bit equality, so two NaNs with different payloads were unequal: a false UNSAT (solver API; the compiler emits no such NaN literals) | sort_check.rs | **fixed** fb57f472 |
| N-PARSE-DIVERGE | S1 | native read scripts differently from z3: quoted symbols typed as literals, multi-query and assertion-stack scripts, `echo`/`set-option`, string escapes, `(- 0.0)`, zero-argument connectives, `(_ bvN w)` with N >= 2^w in shifts, >64-bit shifts, `;`/`|`/`"` inside atoms, sorts outside the declared logic (solver API; each could yield a certified false UNSAT on an external SMT file) | sort_check.rs | **fixed** fb57f472 |
| N-TOKENIZE-HANG | S3 | a stray top-level `)` made the tokenizer loop forever | sort_check.rs | **fixed** fb57f472 |
| N-LOWER-EXP | S3 | lowerings cloned their operands, so nested `fp.*` / Float64 `=` / Boolean `=` / `ite` grew exponentially: a 241-byte query exhausted memory and aborted | lowering_limit.rs | **fixed** fb57f472 (shared, hash-consed operands; linear) |
| N-PARSE-EXP | S4 | `=` tried the term reading before the Boolean one and re-parsed nested operands: exponential parse time (3.2 s at 457 bytes) | lowering_limit.rs | **fixed** fb57f472 |
| N-STACK | S3 | deep nesting overflowed the stack and aborted the process (about 20,000 levels on the main thread, about 256 on a 2 MiB debug thread) | sort_check.rs | **fixed** fb57f472 (own 256 MiB solver stack, cap 1024) |
| F-NEGZERO | S1 | `-0` in the float lane was modeled as -0.0; the runtime has +0.0, so `1.0 / (x * -0) < 0` was proved and the program returned inf | matrix `fl_neg_int_zero_divisor*` | **fixed** fb57f472 |
| F-LOOP-INTWRITE | S1 | `s = 7` in a float loop was modeled as 7.0; the runtime stores Int 7, so `s / 2` is 3, and `ensures(result == 3.5)` was proved | matrix `fl_loop_int_assign_*` | **fixed** fb57f472 |
| F-STRUCTLIT-F64 | S1 | `P { x: 7 }.x` with `x: f64` was modeled as the integer 7; the runtime coerces it to 7.0, and `r == 3` was certified for r = 3.5 | matrix `fl_structlit_*` | **fixed** fb57f472 |
| F-U64-LITERAL | S1 | (introduced and fixed before commit) a u64-range integer literal admitted to the float lane was modeled as about 1.8e19; the runtime wraps it to a negative i64 | matrix `sort_u64_*` | **fixed** fb57f472 |
| F-SNIFF | S3 | an integer identifier containing `to_fp` routed its obligation to QF_FP, ill-sorted, and a valid program failed | matrix `sniff_int_ident_contains_to_fp` | **fixed** fb57f472 |
| X-Z3-ANSWER | S2 | the cross-checks read only z3's first line: a warning line, a z3 that exited before reading a large query, or an error mentioning "out of memory" let a native verdict on a rejected query stand; a z3-rejected vacuity query left the PASS standing; a rejection mentioning `undecided` was classified as a budget limit | z3_rejected_query_fails_closed.rs | **fixed** fb57f472 |
| C-FLOAT-SLICE | S4 | every float fact rode every float obligation, so one unrelated `fp.mul` definition made all float obligations z3-only (uncertified) | relevance_slicing_tests | **fixed** fb57f472 (connectivity plus dangling-definition slicing) |
| R-STR-BYTES | S4 | native reads raw non-ASCII in an SMT string literal as UTF-8 code points and keeps NUL; z3 reads bytes and truncates at NUL. Solver API only: the compiler refuses such literals as unmodelable | review round 2 | open (recorded) |
| R-IFEXPR-FOLD | S4 | a struct-literal field read inside an `if`-expression initializer is not folded, so it is unmodeled (the runtime `assert` still traps) | review round 4 (`c7_let_expr_block`) | open |
| R-FLOAT-DEF-GATES | S4 | NaN-aware Float64 `=` costs extra gates; 700+ chained float lets reach the native gate ceiling and are decided by z3 alone | review round 1 | open |
| ENV-OOM | — | review fuzzers exhausted memory twice; the global OOM killer failed the terminal scope and killed the Claude Code session. Heavy jobs now run in memory-capped scopes (`systemd-run --user --scope -p MemoryMax=…`) | journalctl 15:05 / 16:21 | recorded |


## Callee values and return joins (1b40f653, 2026-09-23)

A call whose callee is a VALUE the resolver cannot name used to be read as "calls no function", so a
contracted function reached that way had its `requires` unchecked. Four adversarial review rounds;
every reproducer is a matrix case (`rv4_*`, `rv5_*`, `rv6_*`, `open_*`, `open_r4_*`).

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-UNKNOWN-CALLEE | S1 | an unresolved callee value was treated as calling no function: `requires` of an escaping contracted function unchecked (c43, c64, c65 and review S1–S10, S14, S15, E7–E17) | matrix | **fixed** 1b40f653: refused UNDECIDED when a contracted function escapes; a formal of the current function is exempt unless assigned or re-bound |
| FV-EARLY-RETURN | S1 | resolvers read only a function's TAIL value, so an early `return g` was invisible (contract and secret/taint lanes, free functions) | `rv4_*`, `rv5_ifc_*`, `rv6_*` | **fixed** 1b40f653 (`fn_return_values`, union on top of the tail answer) |
| FV-CLOSURE-BODY | S1 | a closure's body was never checked when the closure was called through a value or passed to a carrier | S14, S15, E14 | **fixed** 1b40f653 for expression-bodied closures (`discharge_applied_closure`) |
| D9 residual | S1 | a field read through an unknown base ignored the declared field's `secret` qualifier | `d9_unknown_arg_type` | **fixed** 1b40f653; P5 (an unrelated public field of the same name as a secret one is rejected) is the accepted cost |
| FV-R4-BLOCK | S1 | a block- or match-bodied closure is instantiated in the caller's scope: `substitution_complete` and the capture test use `collect_expr_vars`, which does not descend into binding forms, and `discharge_calls_in_expr` defers blocks | `open_r4_*body*`, `open_r4_block_body_via_list`, `open_r4_shadowed_capture_in_expr_block` | **fixed** 8424dc0c: runtime capture set, deep substitution check, block refusal only when the body matters |
| FV-R4-ARITY | S1 | the runtime does not enforce closure arity once a closure is passed, returned or stored, but the checker prunes closures of another arity and reads an unresolved closure callee as "no function" | `open_r4_arity_*` | **fixed** 8424dc0c: arity follows the runtime (missing argument 0, surplus ignored) |
| FV-R4-METHOD | S1 | methods with early returns: `fn_return_values` has no method twin | `open_r4_method_early_return_*` | **fixed** 8424dc0c: `method_return_values` lists every definition (trait defaults included) |
| FV-R4-ALIAS | S1 | secret lane: an early return that goes through a callee local (`return t[0]`) is unresolvable and the tail answer is used | `open_r4_ifc_early_return_callee_local` | **fixed** 8424dc0c: callee stable locals are rewritten through |
| FV-R4-HOF | S1 | a closure passed to a higher-order builtin (`map([-1], \|x\| f(x))`) is never applied | `open_r4_map_builtin_with_closure` | **fixed** 8424dc0c: closures applied; list literals, stable locals and `range` modeled; other collections refused |
| FV-OPEN | S1 | calls through struct fields and map entries (S11, S12), `for` over a formal list (S13, E16), a match-arm expression (S16), whole-struct and string-index secret prints (S17, S18) | `rv7_*` | **fixed** 8424dc0c |
| FV-P7 | S3 | a join's unreachable closure branch is checked without its path condition, so a valid program is DISPROVED (named-function joins behave the same) | `rv6_valid_join_unreachable_closure` | open (fails closed, wrong class) |
| FV-OVERREFUSE | S3 | valid closures refused UNDECIDED: a block body over a stable capture (P9), a capture of the `for` variable called in the same iteration (P10), a formal wrapped in a list (P3) | `rv6_valid_*`, review P3 | open |
| FV-MIXED | S4 | c12, c54, c61 move from DISPROVED to MIXED (an extra explicit refusal next to the counterexample) | matrix | open (still rejected) |

### Follow-up: 8424dc0c (review rounds 4–10)

Matrix (420 cases): silent accepts 115 on anubis-sortchk12, 84 on anubis-esc7 (1b40f653), 18 here;
all 18 are `open_*` and silent on both earlier pins.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-BLOCK-SHADOW | S1 | a value block that re-binds a modeled name was skipped entirely by the expression walker, so a contracted call or a contracted function handed to `map` inside it was never checked | `rv7_block_shadow_call_unchecked`, `rv11_*shadowed*` | **fixed** 8424dc0c (refused) |
| FV-VALUE-POS | S1 | a bare-expression `match`/`if let` arm body and a block's tail were analyzed for effects only, never discharged | `rv7_match_arm_*`, `rv8_match_arm_*` | **fixed** 8424dc0c |
| FV-FIELDCALL | S1 | `obj.h(args)` with no method `h` anywhere was not checked as a call through the field value | `rv7_fieldcall_*` | **fixed** 8424dc0c |
| FV-JOIN-NAMED | S1 | a merged returned closure hid a named-function leaf of the same join (a draft regression) | `rv8_join_*` | **fixed** 8424dc0c |
| FV-REENTRY | S2 | the return-value expansion was exponential on recursion (a shipped example stopped terminating); a guard then hid secrets arriving on deeper frames | `rv8_reentry_secret`, `rv9_reentry_*` | **fixed** 8424dc0c (two frames; guard trip scans for secret names) |
| FV-WHOLE-STRUCT | S1 | printing a whole struct with a `secret` field (directly, in a list, nested, interpolated, `str(..)`, map/enum payload, forwarder, through a formal printed whole) leaked the field | `rv7_whole_struct_*`, `rv8_whole_*`, `rv9_whole_*`, `rv10_whole_*` | **fixed** 8424dc0c (`whole_value_source`, `param_whole_egress`) |
| RT-STRIDX | S1 | RUNTIME: native `p["key"]` on a struct returned its FIRST declared field whatever the key (`index_get`: a non-numeric string key parsed to index 0), so `p["pub_n"]` printed the secret `k` | tools/anubis/tests/struct_string_index_reads_named_field.rs; `rv11_*`, `rv12_strindex_unknown_base` (recategorized ACCEPT) | **fixed** 3b90cd4b (a string key reads the named field; checker back to per-field semantics) |
| RP-ARRAY | S4 | a counterexample whose model held an array was reported REPLAY_MISMATCH: the replay pinned `(= arr #x…)`, ill-sorted | `rv7_arity_closure_in_list`, lib.rs test | **fixed** 8424dc0c (arrays never pinned; can only reclassify a FAIL) |
| FV-OPEN-2 | S1 | `obj.m(..)` where `m` is some other type's method; whole struct via method return, closure return, `push`, `for`/`match`/`if let` binding, map dot access or `values(..)`, or formal routes; alias lane without an unknown state (deep recursion through an assigned local or helper) | `rv14_*` | **fixed** c527a48d |
| FV-OVERREFUSE-2 | S3 | valid programs refused: block re-binding a modeled name that calls a contracted fn (P12), arm call over an enum payload (P13), `map` over a parameter list or a `map`/`filter` result (P15–P17), negative-step / three-argument / aliased ranges (P19–P21), range bound from an untyped formal (P24), map keys named like a secret struct field (P25, P26, until RT-STRIDX), a user `fn push` in expression position (P27), plus P3, P9, P10 | `rv8_valid_*` … `rv12_valid_*` | open (refusals, not silent accepts) |
| RT-STRIDX-SET | S4 | RUNTIME: a struct string-key WRITE `p["k"] = v` is a silent no-op (`index_set` has no struct arm) | review round 11 | open (not a leak) |
| RT-POSKEY | S4 | RUNTIME: a non-string, non-integer key on a struct is positional (`p[true]`, `p[1.0]` read field 1); safe only because the checker treats any non-string struct index as the whole value | review round 11 | open |
| FV-ASSIGN-WHOLE | S3 | whole-struct assignment tracking marks the whole root and never clears it: `xs[0] = p; print(xs[1])` and an overwritten entry are refused | `rv13_valid_assign_*` | open (precision) |

### Follow-up: c527a48d (review rounds 12–20)

Matrix (489 cases): silent accepts 161 on anubis-sortchk12, 68 on anubis-stridx1 (3b90cd4b), 16 here;
all 16 are `open_whole2_*` and silent on both earlier pins.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-WHOLE-ROUTES | S1 | a whole struct with a secret field reached `print` through bindings (`for`, `match`, `if let`, `push`), returns (functions, methods, closures, `wrap(s) = [s]`), map dot access / `values`, and formals released through methods or closures | `rv14_whole_struct_*`, `rv14_map_*` | **fixed** c527a48d |
| FV-FIELDCALL-2 | S1 | `obj.h(..)` where `h` is a method of some type but the receiver's (possibly unknown) type holds a function in field `h`: the runtime calls the field's value | `rv14_fieldcall_method_name`, `rv15_*`, `rv16_*`, `rv17_*`–`rv20_*` field cases | **fixed** c527a48d (least-fixpoint `fn_fields` over every write, incl. `map_values`) |
| FV-ALIAS-UNKNOWN | S1 | the secret/taint alias lane fell back to the tail answer when a return value could not be rewritten or the re-entrancy guard stopped, losing secrets through assigned locals, helper chains, methods, pushes and map writes | `rv14_reentry_*`, `rv15_reentry_*`, `rv17_helper_*`, `rv19_helper_*`, `rv20_helper_*` | **fixed** c527a48d (`scan_for_secret_fn`, `return_feeding`, `helper_labeled`) |
| FV-OPEN-3 | S1 | still open, pre-existing: 16 whole-struct routes from review round 12 (closure parameters, method formals, let-patterns, list builtins passing elements through, a local list returned) | `open_whole2_*` → `rv24_whole2_*` | **fixed** 9aadfe41 for 15 of 16; `open_whole2_B2` (a closure returned from a closure, `let g = mk(); g()`) still open |
| FV-OVERREFUSE-3 | S3 | an untyped formal stored in a field may be a function, so a method call named like that field on a receiver of unknown type is refused when a contracted function escapes | `rv17_valid_builder_formal_plain_value` | open (documented) |
| PERF-FANOUT | S3 | check time on mutually recursive fan-out (4 functions × 3 recursive calls ≈ 10 s; 40 ≈ 33 s) since the re-entrancy guard (8424dc0c); refused on every pin, time only | review round 13 T1, rounds 22–25 | **fixed** 6fe90617 (one frame per function, a per-function budget on deep expansion, an exact memo; 40 functions ≈ 5 s) |
| RT-LAMBDA-FNNAME | S3 | RUNTIME/lowering: a closure naming a user function or builtin as a value (`\|v\| f`) failed to compile (E0425) although `check` accepted it | tools/anubis/tests/closure_names_function_value.rs | **fixed** d70eb2ed; residual: a name that is also a `let` local elsewhere in the function still fails to compile (fails closed) |

### Follow-up: d70eb2ed, 6fe90617 (review rounds 21–25)

Matrix (509 cases): silent accepts 173 on anubis-sortchk12, 20 at anubis-perf4 (6fe90617), all
`open_*` and silent on every earlier pin.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-ALIAS-SCAN | S1 | the secret lane's over-approximating scan missed a secret held in a caller local, a helper handed in as an argument, and anything past an expansion cut-off | `rv21_*`, `rv22_*`, `rv23_*` | **fixed** 6fe90617 |
| FV-OPEN-4 | S1 | still open, pre-existing: a closure capturing a secret function passed as a formal; a local reassigned to a secret function after its use in a loop; a struct field set to a secret function after an earlier use; a closure returning a secret function value | `open_reentry_closure_capturing_secret`, `open_loop_reassign_after_use_secret`, `open_struct_field_reassigned_after_use_secret`, `open_closure_returning_secret_fn` | **open** |
| DIAG-ARITY-SELFFIELD | S4 | a method returning `self.g` whose result is called gets a false ANUBIS_ARITY_MISMATCH naming the callee; fails closed | review round 25 B1 | open |

### Follow-up: 9aadfe41 (review round 26)

Matrix (512 cases): silent accepts 175 on anubis-sortchk12, 22 on anubis-perf4 (6fe90617), 5 at
anubis-whole10 (9aadfe41): `open_whole2_B2` and the four FV-OPEN-4 cases, all silent on every
earlier pin. No new wrong-class case.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-OPEN-5 | S1 | pre-existing on every pin: ten more whole-struct pass-through shapes found by review round 26 — the builtins `drop`, `chunk`, `window`, `enumerate`, `zip`, `repeat`, `reduce` (two forms), `apply`, and a block-bodied lambda aliasing its parameter | `rv25_*`, `rv26_*` | **fixed** 0731ecf7 |

### Follow-up: 0731ecf7 (review rounds 26–35)

Matrix (1108 cases): silent accepts 578 on anubis-sortchk12, 295 on anubis-whole10 (9aadfe41), 23 at
0731ecf7: the `open_*` cases below, all silent on every earlier pin. Wrong-class 76 (42 on
anubis-whole10): the 36 new ones are the documented refusals in OR-WHOLE.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-WHOLE-LANE | S1 | the whole-struct lane was seven hand-maintained walkers with separate function summaries: a builtin, operator or unresolved name they did not list carried nothing (45 routes: review round 26 and a sweep of the runtime's builtins) | `rv25_*`, `rv26_*` | **fixed** 0731ecf7 (`middle/whole.rs`) |
| FV-WHOLE-REWRITE | S1 | the rewrite's own defects in its uncommitted versions, found by review rounds 27–35 (leaks and over-refusals, each reproduced by an independent verifier) and fixed before commit. Round 32's 20 leaks were mostly round-31 regressions: method dispatch by a scope binding's declared or inferred type alone (stale after a reassignment, an element write, or a helper's declared return type), a value that may itself be the struct losing that under the size or depth cap, a captured closure-or-value losing its closure half, a statement `print` hidden by a user function of that name, a builtin name shadowed by a closure, a function moved along a chain of names in an unsettled loop, a declassified struct's dispatch. Rounds 33–35 (24, 39, 40): a type made stale through a derived name, a struct-form `Err` losing its names, main-scope loops carrying a struct to the next iteration, released struct types, function values handed back by builtins, and the over-refusals their first fixes caused | `rv27_*` … `rv35_*` (531 cases) | **fixed** 0731ecf7 |
| FV-OPEN-6 | S1 | still open, pre-existing: calling a closure fetched from a container, chosen by an expression, reassigned in a branch, or built by `compose`; `panic` as an output channel | `open_whole4_w11_ip_*`, `open_whole5_r28_*` | **open** (callbacks unit) |
| IFC-PC-EGRESS | S1 | still open, pre-existing: implicit flow — an assignment, egress or `return` under a condition a whole struct decides, including a `return` selected by such a `break` | `open_whole4_l14_*`, `open_whole4_l16_*`, `open_whole6_r29_s4_*` | **open** (a policy decision: egress under a secret condition is accepted today) |
| FV-ENUM-SECRET | S1 | still open, pre-existing on every pin: an enum whose variant DECLARES a secret payload reaching an egress whole (`print(E::A(k))`), and a struct-form variant's field declared secret read directly (`e.v`, symmetric in the ordinary lane). The whole-struct lane covers structs only | `open_whole11_r34_l_enumsecret_*` | **open** (its own unit) |
| IFC-LOOP-CARRIED-VALUEBLOCK | S1 | still open, pre-existing, ordinary lane: a value carried to the next iteration of a loop inside a main-scope value block, or into a `while let` binder, reaches an egress earlier in the body (the whole-struct lane now seeds loops from its own fixpoint; the scalar lane's seeding does not cover these positions) | `open_whole11_r34_*_symmetric` | **open** |
| L-SHADOW-1 | S2 | still open: a write to an outer name inside a branch that then shadows it is symmetric between the lane and the runtime (`open_whole8_r31_r31i_s1`) | `open_whole8_*` | **open** (a language decision) |
| TY-UNENFORCED | S2 | declared types are not enforced (`let s: list<i64> = "ab"` checks). The whole-struct lane uses them only to add what a value may hold, and dispatches a method call by a declared or inferred receiver type only together with the secret struct types the value names, and never when the name may have changed type (assigned a value not known to keep the type, written into, or bound from one that was) | review rounds 29 and 32–35 | open (checker) |
| OR-WHOLE | S3 | documented over-refusals, fail-closed: a helper built by `compose` or returning a closure, called with such a struct; `x + [a]` where `x` is not known to be a list from its shape; more than 24 nested helpers; closures rotated through recursion; `drop` / `skip` forget positions; a list mixing a struct and its rendering, read by element field; an `Err` arm binding what an `Ok` payload holds; a map returned by a helper then written and read by different string keys; a method call whose receiver carries no struct type (a method's result, a loop variable or match binder over secret-less literals, `self.field`) runs every impl of a type without a secret field, and one on a name whose type may have changed runs those too; a helper receiving 33 or more distinct closures in one query; a closure rebuilt each iteration of a loop; the step budget; a loop (main-scope ones included) whose names need more than 24 iterations to settle; a join over what a constant condition decides, evaluation order or `continue` paths; nesting builtins forgetting positions; a key rewrite adding a key | the 36 `rv27_valid_*` … `rv35_valid_*` wrong-class cases | open (documented) |
| PERF-WHOLE-NEST | S3 | the interpreter's cost multiplies by 3–5 per nesting level of loops that rebuild containers or closures (a 9-deep nest: 20 s; a 6-deep nest of closure rebuilding: refused after 12 s by the step budget). Memory is bounded (256 remembered block runs per query; a 9-deep nest went from 2 GB to 130 MB). Main-scope loops now run the interpreter too: a nest rotating records through many names takes about 3 s (100x the pre-rewrite pin), up to 13 s, bounded by the step budget | `rv32_valid_p1_*`, `rv32_p2_*`, `rv35_valid_r35i_10_perf_mainnest` | open (documented) |

### Checker limits: 66ac16ce (analysis_limit)

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| CHK-STACK-OVERFLOW | S1 | a closure that reaches itself through a reassigned name (`g = |s| g(s)`), or a chain of closures whose bodies nest deeply, overflowed the checker's stack (SIGABRT 134, every pin since the closure analyses). Found by the crash-diagnosis Codex sessions of 2026-09-24, which contributed the request-level refusal, its recovery between requests and 7 fixtures | `limit_recursive_closure_source`, `tests/fixtures/language_core/closure_analysis_*` | **fixed** 66ac16ce: `middle/analysis_limit.rs` — a stack guard at every recursive walker entry (1 MiB red zone), at most 4096 nested descents that follow a name to a closure, sticky per request, `ANUBIS_ANALYSIS_LIMIT`; the command line checks on a 64 MiB thread |
| CHK-MEMORY | S1 | analyses that copy or substitute expressions grew without bound on valid source (the contract carrier's inlining of never-assigned `let`s, argument substitution along forwarder chains, quadratic child copies in the carrier visitor, alias substitution, `walk_block_effects`' scope copies): the kernel's OOM killer then ended the check and its session | reviews of crashfix1 and crashfix5 (`l*`, `s*`, `cc_sexp24`, `hc_20`, `flat_*`, `mkq_2000`) | **fixed** 66ac16ce: a counting allocator bounds every check (`crate::resource`); the contract carrier bounds each resolution at 4096 nodes (past it, the alias placeholder); the other analyses still cost what they cost (their blowups are now refusals) |
| CHK-DEEP-AST | S2 | operator and call chains are parsed in loops, so the parser's nesting bound (which keeps every later pass within its stack) did not limit their depth, and the contract carrier inlined `let`s into trees deeper than any source: a 60000-call curried chain, a 100000-term sum and 1500 shadowing `let`s each 256 deep aborted the checker (SIGABRT) while it copied a tree | review of crashfix5 (`cur_60k`, `sh_1500`, `flat_100k`); `limit_chain_sum_10000`, `limit_chain_call_10000` | **fixed** 66ac16ce: at most 8192 operator and postfix links on one path through an expression (`MAX_CHAIN`, counted across nesting; one diagnostic), and the carrier's resolution bound |
| PERF-LONG-SUM | S3 | still open, pre-existing: a sum of n terms costs the checker time and memory quadratic in n (1000 terms 21 s; 3000 terms 50 s and 2.1 GB, where the previous pin aborted), so a long sum under a small memory budget is refused with `ANUBIS_ANALYSIS_LIMIT`. Each `+` is its own wrap-safety obligation stated over the whole prefix (n obligations of size up to n), and the report prints each in full (608 KB for 400 terms) | `tools/anubis/tests/analysis_memory_limit.rs` | open |
| GATE-FIXTURE-NEEDLE | S2 | the language fixture runner matched `ERROR_CONTAINS:` against every file in the fixture's output directory, including the AST dump that carries the fixture's own header, so every FAIL fixture with a needle passed on any failure; then (first fix) against the command line's echo and paths naming the fixture; five needles were general enough to match unrelated failures. One fixture's needle was stale (`float_let_frame_leak_rejects`: DISPROVED, not FLOAT_CONTRACT_UNMODELED; the counterexample it prints, x = NaN, does not satisfy `requires(x > 20.0)`: FLOAT-CEX-OUTSIDE-REQUIRES) | `scripts/run_language_fixtures.sh` | **fixed** 66ac16ce (checker output only; needles are extended regular expressions) |

### Follow-up: d8404410 (fourth review of the checker limits)

Eight findings on crashfix7, each re-run by a verifier. Three were already fixed by 66ac16ce (the
curried-chain and shadowing-`let` aborts, and the 100000-term sum: CHK-DEEP-AST).

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| CHK-BUDGET-OR | S2 | 66ac16ce's memory budget (a sixth of the cgroup or physical limit, at most 4 GiB) refused valid programs that fit in the machine several times over: under a 1500 MB scope a 600-branch `else if` needing 294 MB, 2000-piece string builds, 3000 straight-line `let`s, closure stress programs (the pin before the limits accepted them all), so a check's verdict depended on the machine far more than it had to | review P1-P4 (`elifs_600`, `strcat_2000`, `manylets_3000`, `cbhell_d_50x20`) | **fixed** d8404410: budgets from the memory free (min of MemAvailable and the cgroup `memory.max`), soft half, hard two thirds; the runaway inputs are still refused |
| CHK-LIMIT-REPORT | S3 | a limit refusal also listed the errors of the fail-closed answers behind it (a secret "past the analysis limit" in a program with no secret), and `--message-format json` classed it `program` / `repair_program`, telling an agent to edit a valid program | review P3, P1 | **fixed** d8404410: the limit is reported alone, as `capability` / `restate_or_raise_budget` |
| CHK-EVIDENCE-RERUN | S4 | the rejection evidence of a limit refusal re-ran the whole analysis, so a refusal took up to 2.2 times as long as the accept before the limits | review P4 (`hf_d_500`) | **fixed** d8404410 |
| CHK-PARSE-RENDER | S2 | parse-error rendering repeated each error's whole source line, outside any budget: a 6 KB malformed file produced 49.6 MB of text, 3000 nested blocks a 1.3 GB allocation that the scope's OOM killer ended | review `vblock_400`, `vblock_3000` | **fixed** d8404410: 20 errors rendered, the rest counted; lines windowed to 80 characters each side |
| PERF-FNCHAIN | S3 | still open, pre-existing on every pin: a chain of 8000 functions, each calling the previous one, takes the checker past two minutes (superlinear interprocedural passes); CPU time is not bounded by the limits, which bound stack and memory | review `fnchain_8000` | open |

### Follow-up: eac6baf7 (fifth review of the checker limits)

Eight findings on crashfix10 (d8404410), each re-run by a verifier: three introduced by d8404410's
reporting change, five pre-existing reporting gaps.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| CHK-LIMIT-HIDES | S1 | d8404410 reported a limit refusal alone, so a genuine finding the limit never touched (a secret exfiltration beside a self-calling closure) vanished from the output, the JSON and the evidence; the verdict stayed a refusal | review R1 (`leak_cycle`) | **fixed** eac6baf7: only diagnostics naming the fail-closed stand-in are dropped; findings follow the limit, and in JSON are their own diagnostic |
| CHK-LIMIT-ADVICE | S3 | one message and one JSON class for three limits: a closure-depth or stack refusal advised raising the memory budget, which cannot change it | review R2 (`self_cycle`) | **fixed** eac6baf7: the refusal names its limit; only the memory one mentions ANUBIS_ANALYSIS_MEMORY_MIB |
| CHK-LIMIT-DETECT | S3 | the evidence bundle recognized a limit refusal by a substring, so a counterexample naming a variable `ANUBIS_ANALYSIS_LIMIT` (or a comment beside a parse error) skipped the bundle's analysis and recorded a false cause | review R3 (`name_assert`, `comment_parse`) | **fixed** eac6baf7: by the leading code |
| CHK-PCA-RERUN | S4 | pre-existing: a limit refusal's PCA claim re-ran the whole analysis (a refusal cost two analyses) | review R4 | **fixed** eac6baf7 |
| CHK-JSON-PARSE | S3 | pre-existing: the JSON parse lane located each error by walking the source from its start (150000 errors: 52 s, 59 MB), and `parse_source` joined every message (an 8.6 MB evidence field for a 480 KB file) | review R5 (`longerr_*`) | **fixed** eac6baf7: a line index; 200 diagnostics and 200 joined messages, then a count |
| CHK-HARD-EXIT-JSON | S3 | pre-existing: the hard memory exit left `--message-format json` output empty (indistinguishable from a clean run to a consumer counting findings) and its message implied ANUBIS_ANALYSIS_MEMORY_MIB was set | review R6 (`elifs_1000` at 600 MB) | **fixed** eac6baf7: a JSON refusal prepared before the check is written on that exit; no evidence bundle can be written there (documented) |
| CHK-VERIFY-LIMIT | S3 | pre-existing: `verify` re-derives a bundle's claim under the analysis budget; on a busier machine a genuine bundle printed "bundle valid: false", indistinguishable from tampering | review R7 | **fixed** eac6baf7: it reports that the claim could not be re-derived (an error); a tampered bundle still never verifies |
| CHK-RENDER-CONTROL | S3 | pre-existing: a parse error's rendered source line copied control characters verbatim, so a file's escape sequences reached the terminal or CI log of whoever checked it | review R8 (`h6_escape`) | **fixed** eac6baf7: escaped (`\u{1b}`), the caret still under its column |

### Follow-up: b72244c7 (review round 36)

Review round 36 of the whole-struct lane (pin whole12h) confirmed 24 findings (18 leaks, six of them
symmetric with the ordinary lane; 3 over-refusals; 3 costs); a design pass and a cross-check added
three pre-existing leaks. Matrix at b72244c7: 1145 cases, 1041 PASS, 29 silent accepts (all `open_*`),
75 wrong-class.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-WHOLE-R36 | S1 | round 36's leaks: a specialization shared by a sure and a merely declared receiver of one type; `?` on a user enum read as unwrapping, and `?` early returns ignored by the accumulator rule; a join's inferred receiver type trusted; closures chosen by expressions reading their names at the call; a use_scope closure's unwritten path joining with nothing; function values dropped at a name that also held a value, at identity sets of several functions, through call/apply, block last statements and arms; apply past 64 positions; a loop whose step budget was spent before its first iteration | `rv36_*` (17) | **fixed** b72244c7 |
| OR-WHOLE-R36 | S3 | round 36's over-refusals and costs: max of a struct literal; a field declassify releasing every secret type; a method name shared by two types losing the accumulator's type; staleness exponential in the binding graph (2^24 ladders, factorial cliques) | `rv36_valid_*` (5) | **fixed** b72244c7 |
| TY-SHADOW-RETYPE | S1 | still open, pre-existing: the retype tables are keyed by name, so a later `let` shadowing a name changes the declared type read for an earlier binding's part or receiver (and, as an over-refusal, a name bound again under another type is stale) | `open_whole13_rv36x_sp2`, `open_whole13_rv36x_rs1` | open |
| FV-OPEN-6 (container, shadow) | S1 | still open, pre-existing: a closure fetched from a list or map whose captured name is shadowed before the call | `open_whole13_rv36x_c1`, `open_whole13_rv36x_c2` | open |
| IFC-ORDINARY-RETURN | S1 | still open, pre-existing, ordinary lane: a field-read secret assigned in a nested block of a callee and returned is lost; a call through a joined function alias checks one of its functions | `open_whole13_r36_r36_spec_3`, `open_whole13_rv36x_oh3` | open (its own unit: a fix to the returns alone turns `rv36x_oh1` and oh2, rejected today, into accepts) |
| PERF-STEP-SIZE | S3 | still open: the interpreter's step budget counts steps, not the size of the values each step copies (a 201-entry record in 22 names: 15 s a query) | review round 36 spec-4 | open (documented) |

### Follow-up: 0275f5f3, efed68bb (sixth review of the checker limits; CI)

Twelve findings on crashfix11 (eac6baf7), each re-run by a verifier.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| CHK-LIMIT-POSITION | S2 | eac6baf7 filtered the diagnostics a limit leaves behind by their text: an artifact that did not quote the stand-in (a public binding "initialized from a secret value" in a program with no secret) was reported as a finding, and a genuine one quoting user text equal to the stand-in (a map key) was hidden | review F1 (`stp_*`), F2 (`k4_hide`) | **fixed** 0275f5f3: by position (`SemanticContext::push_diag` marks where the first one after the limit begins) |
| CHK-LIMIT-REACH | S3 | a finding in a part of the program the analysis did not reach before the limit is not reported | review F4 | documented: that part was not analyzed; the program is refused regardless |
| CHK-VERIFY-ORDER | S2 | eac6baf7's verify returned "could not be re-derived" before its tamper checks, and for a forged claim over a program past the closure-depth cap (the same on every machine) | review F3 | **fixed** 0275f5f3: integrity first; only a MEMORY limit gives that answer |
| CHK-SCOPE-SIDE-BY-SIDE | S1 | pre-existing: checks run side by side in one memory-capped scope each armed with the whole scope's headroom, and the scope's OOM killer ended them all with no output before any budget refused | review F8, B1 | **fixed** 0275f5f3: budgets from what the tightest cgroup has left; the allocator reads that again every 64 MiB, without allocating, and refuses below a reserve |
| CHK-LIMIT-JSON | S3 | a stack or closure-depth refusal carried the same machine-readable action as a memory one (raise the budget), and its text named a cycle the program may not have | review F5, F6 | **fixed** 0275f5f3: only a memory refusal carries a `budget`; the texts say which limit and that memory does not change the others |
| CHK-PATH-LIMIT | S4 | a parse rejection of a file named `ANUBIS_ANALYSIS_LIMIT` was taken for a limit refusal | review F7 | **fixed** 0275f5f3 |
| CHK-CONTROL-SEMANTIC | S3 | pre-existing: semantic diagnostics quoting user text (a map key) reached the terminal with its control characters | review F9 | **fixed** 0275f5f3: human output escapes them |
| CHK-LSP-PARSE | S3 | pre-existing: the LSP located each parse error by walking the source (60000 errors: 16.5 s, 11 MB) | review F10 | **fixed** 0275f5f3: a line index, 200 errors then a count (23 ms, 36 KB) |
| CHK-COVERAGE-SIZE | S3 | pre-existing: the coverage report named every obligation by its whole SMT term (a 1000-term sum: 8.5 MB of JSON) | review F11 | **fixed** 0275f5f3: 100 names of at most 240 characters, and a count (25 KB) |
| CI-MACOS-CLIPPY | S3 | the hosted gate (macOS) failed clippy: Linux-only memory readers were dead code there | CI run 36090194164 | **fixed** efed68bb |
| CI-MACOS-FIXTURES | S3 | the hosted gate (macOS) fails the language fixtures; the cause is not known here (the gate's log stays on the runner) | CI run 36090194164 | open: efed68bb prints every failing gate's log end into the job log |

### Follow-up: 9dc7be91 (IFC-ORDINARY-RETURN)

The ordinary lane's return summaries (secret and taint) and calls through a joined function binding.
Matrix at 9dc7be91: 1207 cases, 1094 PASS, 34 silent accepts (all `open_*`), 79 wrong-class.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| IFC-ORDINARY-RETURN | S1 | pre-existing: the return summary behind secret_fns / tainting_fns (body_returns) restored the scope after a nested block, so a secret or tainted value a branch, loop, match arm or value block assigned to an outer binding and then returned was lost; a `for` variable or `while let` binder took nothing from its header | `open_whole13_r36_r36_spec_3`, `rv36o_*` (26 leaks, 21 valid twins) | **fixed** 9dc7be91 |
| IFC-ALIAS-UNION | S1 | pre-existing: a call through a local binding that may hold several functions (a join) checked the egress, sinks and returned parameters of one preferred name; fixing the return summary alone would have turned two rejected leaks into accepts | `open_whole13_rv36x_oh3`, `rv36x_oh1`, `rv36o_alias_*` | **fixed** 9dc7be91 |
| EX-VAULT-CONTACTS | S3 | the vault_contacts example sent its file data (tainted) to print and save_vault through a loop the return summary did not see | both copies of vault_contacts.anb | **fixed** 9dc7be91: declassify at the parse boundary |
| OR-ORDRET-ORDER | S4 | documented over-refusal: inside one nested statement, writes are order-free and names conflated (a label cleared later in the branch, an inner shadow, a read before a later write, another match arm's write) | `rv36o_or_*` (4, wrong-class) | documented |
| IFC-JOIN-BREAK-SHADOW | S1 | still open, pre-existing: the enforcing lane's and the value-block lane's joins (merge_taint_over) keep only end states and skip paths that shadow the name; the parameter-return summary (body_param_returns) has the same two holes | `open_rv36o_sib_*` (7) | open |
