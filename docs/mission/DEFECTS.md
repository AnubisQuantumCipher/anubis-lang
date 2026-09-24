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
| FV-OPEN-2 | S1 | still open, all pre-existing: `obj.m(..)` where `m` is some other type's method (`open_fieldcall_method_name`); whole struct via method return, closure return, `push`, `for`/`match`/`if let` binding, map dot access or `values(..)`, or formal routes `param_whole_egress` does not follow (`open_whole_struct_*`, `open_map_*`); the alias lane has no unknown state, so deep recursion can still hide a secret via an assigned local or helper call (`open_reentry_*`) | `open_*` | **open** |
| FV-OVERREFUSE-2 | S3 | valid programs refused: block re-binding a modeled name that calls a contracted fn (P12), arm call over an enum payload (P13), `map` over a parameter list or a `map`/`filter` result (P15–P17), negative-step / three-argument / aliased ranges (P19–P21), range bound from an untyped formal (P24), map keys named like a secret struct field (P25, P26, until RT-STRIDX), a user `fn push` in expression position (P27), plus P3, P9, P10 | `rv8_valid_*` … `rv12_valid_*` | open (refusals, not silent accepts) |
| RT-STRIDX-SET | S4 | RUNTIME: a struct string-key WRITE `p["k"] = v` is a silent no-op (`index_set` has no struct arm) | review round 11 | open (not a leak) |
| RT-POSKEY | S4 | RUNTIME: a non-string, non-integer key on a struct is positional (`p[true]`, `p[1.0]` read field 1); safe only because the checker treats any non-string struct index as the whole value | review round 11 | open |
| FV-ASSIGN-WHOLE | S3 | whole-struct assignment tracking marks the whole root and never clears it: `xs[0] = p; print(xs[1])` and an overwritten entry are refused | `rv13_valid_assign_*` | open (precision) |
