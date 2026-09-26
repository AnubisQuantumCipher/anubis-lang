# Mission defect inventory (from the Stage-A subsystem review)

Derived from `docs/mission/review/01-04`. Each row is a defect or gap found by reading the source;
"verified" means confirmed by a probe or by an exact code citation, "reported" means asserted by the
mapper and not yet independently reproduced by the integrator. This inventory feeds `docs/CLAIMS.md`
(soundness defects) and the roadmap; it is not a second status authority.

Severity: S1 unsound/false-claim in a shipped assurance; S2 gate/evidence that can pass without asking
its question; S3 precision/over-rejection or crash on input; S4 correctness/quality; S5 docs/hygiene.

## Alias candidate verification (2026-09-25; not shipped)

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| CAND-ALIAS-STACK | S3 | Workspace release tests abort in `recursive_closure_source_reports_limit_and_recovers` on the uncommitted alias candidate. Matching baseline differential is not yet measured. | [Complete failure receipt](../evidence/ALIAS_CANDIDATE_VERIFY_2026-09-25/README.md), source patch, manifest and full cargo log; cargo rc 101. | open; blocks candidate integration; follow-up deliberate crash verification requires the mandated guest |
| CAND-CLOSURE-CAPTURE | S1 | An unshipped closure-effect worklist suppresses repeated current-scope states without traversing the old captured printer; invalid self/mutual-assignment controls become accepted. | [Rejected-design receipt](../evidence/ALIAS_CLOSURE_WORKLIST_REJECTED_2026-09-25/README.md); `ord3_capture_*` matrix controls; native output changes with the annotated secret. | prototype rejected; no compiler change integrated; capture-versioned ordinary effects remain required |

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

### Follow-up: 2536d046 (seventh review of the checker limits)

Eleven findings on crashfix13 (efed68bb, the code of 0275f5f3), each re-run by a verifier. None is an
acceptance: in no probe did a check pass after a limit, and no tampered or forged bundle verified.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| CHK-CACHE-ACTIVE | S3 | 0275f5f3 counted only a cgroup's inactive file cache as free: a scope holding page cache read twice refused valid programs (interp_1400 in 1000M with 734 MB of cache: a 137 MiB budget) | review N2, BUDGET-1 | **fixed** 2536d046: active and inactive file cache count as free |
| CHK-CACHE-LIVE | S2 | pre-existing: the cache was counted once, at the start, so the headroom looks overstated what was left as the kernel reclaimed it, and checks side by side in a scope holding page cache were killed together again | review N3 | **fixed** 2536d046: read at every look |
| CHK-LOOK-COARSE | S2 | pre-existing: a look every 64 MiB, before the pending allocation, let two or three checks in a 600M scope (or two with doubling arrays in 1000M/1500M) overshoot the cap together | review BUDGET-2 | **fixed** 2536d046: a look every eighth of the reserve (4 to 64 MiB) and before any allocation that large, against what would be left after it |
| CHK-EVIDENCE-STARVED | S3 | 0275f5f3: `check --evidence` re-checks the program in the same process, armed from headroom net of the first check's own memory: a valid program passed, then "verdict: FAIL", an agent told to repair it, and its bundle called tampered | review N1 | **fixed** 2536d046: a request takes at least what the process's first could use |
| CHK-RESERVE-REPORT | S3 | 0275f5f3: the reserve's exit claimed a hard budget the check had not reached; no memory exit's JSON named a budget; setting ANUBIS_ANALYSIS_MEMORY_MIB, as advised, turned the reserve off | review N4 | **fixed** 2536d046: the reserve's own text and report; the hard exit names its budget; the reserve stays on |
| CHK-INDEPENDENT-FINDINGS | S3 | 0275f5f3: findings that need no analysis (a duplicate parameter, a `break` outside a loop) were dropped with the rest after a limit | review N5 | **fixed** 2536d046 |
| CHK-VERIFY-HARD-EXIT | S3 | pre-existing: verify re-derived before deciding integrity, so a tampered bundle whose re-derivation reached the hard budget ended with the check's exit text and no verdict | review N6 | **fixed** 2536d046: integrity first |
| CHK-VERIFY-KEPT-FINDING | S3 | pre-existing: a forged PASS over a program with a finding made before the memory limit answered "could not be re-derived" at every memory size | review N7 | **fixed** 2536d046: refuted |
| CHK-CONTROL-VERIFY | S3 | pre-existing: verify printed a bundle's escape-laden field names raw, and `report` a bundle's text; bidirectional overrides were never escaped | review N8 | **fixed** 2536d046: every printed error and report goes through printable(), which also shows bidi and zero-width characters |
| CHK-COVERAGE-NAMES | S4 | 0275f5f3: 100 names cut to 240 characters could all read the same | review N9 | **fixed** 2536d046: first 160 and last 60 characters and a digest |
| CHK-DISARM | S4 | 0275f5f3: after a request the allocator kept looking and could exit outside any check | review note | **fixed** 2536d046 |

### Follow-up: 28d21d21 (review of 9dc7be91; CI)

An independent review of 9dc7be91 (IFC-ORDINARY-RETURN, pin anubis-ordret1 against anubis-crashfix13)
confirmed 33 findings, each re-run by a verifier. The 30 with a verdict are matrix cases since 28d21d21
(1237 cases at that commit: 1094 PASS, 58 silent accepts, 85 wrong-class). Fix designs are in progress.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| IFC-ALIAS-PREFERENCE | S1 | REGRESSION of 9dc7be91: a call through a function binding whose identity set is Unknown checks the one preferred alias; making a function secret-returning flipped that preference, so the other function's egress, or its declared capability, went unchecked | `open_ro1_alias_regression_unknown_set_secret`, `open_ro1_alias_regression_unknown_set_capability` | open (fix in design) |
| IFC-ALIAS-RESOLUTION | S1 | pre-existing: an unresolvable branch makes a binding's whole identity set Unknown; value-block tails, value-position assignments, lambda-and-name joins, closures returning functions and call-result field types resolve one name or none; a local named like a user function is taken for the callee although the runtime calls the user function | `open_ro1_unknown_set_*`, `open_ro1_value_block_tail_identity`, `open_ro1_fn_assigned_in_value_block_lost`, `open_ro1_lambda_plus_named_fn_join_skips_closure`, `open_ro1_closure_returning_function`, `open_ro1_struct_type_single_alias_at_join`, `open_ro1_*local*` (3) | open |
| IFC-SUMMARY-GAPS | S1 | pre-existing: the return summary does not track which function a local holds; push/insert in expression position; `?` as an early return (also under a secret condition); a function stored into a field inside a branch; four symmetric shapes (a read after a value-block write in one expression, a lambda assigning a captured name, a secret key in an assignment target, a mistyped struct annotation) | `open_ro1_ret_*`, `open_ro1_implicit_flow_try_under_secret_branch`, `open_ro1_enforcing_branch_join_drops_field_fn_identity`, `open_ro1_sym_*` | open |
| IFC-HOF-NAMED | S1 | pre-existing: a named function given to a builtin HOF through a local, or to a user HOF, is not checked; a builtin HOF does not charge the function's capability; a loop reassigning a function after its call | `open_ro1_hof_*`, `open_ro1_user_hof_named_fn_arg`, `open_ro1_loop_reassign_after_call_egress` | open |
| OR-ORDRET-CLOSURE | S3 | introduced by 9dc7be91: the order-free closure cannot clear a label that a later statement overwrites on every path, or that an entering loop overwrites before reading it, and it carries three older imprecisions into branches and loops | `ro1_or1`..`ro1_or6` (wrong-class) | open (fix in design) |
| PERF-ORDRET-CLOSURE | S3 | introduced by 9dc7be91: the closure is recomputed at every nesting level and its fixpoint is quadratic in a reverse carry chain (three programs 6.5-14x slower than the previous pin, over 1 s) | review perf findings | open (fix in design) |
| CI-MACOS-FIXTURES | S3 | the hosted gate's language fixtures failed on macOS since 66ac16ce: the needle search used GNU-only sed syntax and hid its error, so the text searched was empty | CI run 36097287313 (log tail); a BSD-sed shim reproduces 131/271 exactly | **fixed** a4ca63c5 |
| CI-G19-PUSHDIAG | S3 | G19 looked for the literal `ctx.diagnostics.push`, which 0275f5f3 replaced by `push_diag` | CI run 36097287313 | **fixed** a4ca63c5 |

### Follow-up: 8ea94c78 (review round 37)

Review round 37 of the whole-struct lane (pin whole13b) confirmed 27 findings: 16 leaks (2 regressions
against the lane before round 35), 8 over-refusals and 3 costs. Six fix designs and a cross-check;
all six landed, merged. Matrix at 8ea94c78: 1286 cases, 1131 PASS, 65 silent accepts (all `open_*`),
90 wrong-class.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| FV-WHOLE-R37 | S1 | round 37's leaks: an unsettled loop dropped a binding's value or function half (2 regressions); the main-scope boundary trusted one closure or function of a choice, dropped names the chooser binds, a block-local alias, reduce's list and functions written in expression position; function values through match arguments, nested call/apply, reduce seeds and builders; a scalar builtin's name bound locally; a `let` rebinding its own name; container joins of other element types; `self` found by name; a written part through an alias | `rv37_*` (16), `rv37x_w1`, `rv37x_d_copy`, `rv37x_pay_mixed`, `rv37x_enum_mixed` | **fixed** 8ea94c78 |
| OR-WHOLE-R37 | S3 | round 37's over-refusals: join leaves that are binders or block-local lets, lookups and declared indexes, held fields of constructor formals, counter maps and map_values, copy-then-mutate accumulators | `rv37_*or*` (8), `rv37x_a_or0_*` (2) | **fixed** 8ea94c78 |
| PERF-WHOLE-R37 | S2 | round 36 ran nested value blocks from every combination of bound and unbound enclosing names (13 wrapper levels: 32.5 s; a memory-limit refusal of a valid program) | `rv37_r37_p1*`, `rv37_r37_p2*` | **fixed** 8ea94c78 (P2's seeding cost documented) |
| TY-SHADOW-RETYPE | S1 | the two recorded leaks (a later `let` shadowing a name changes the type read for an earlier binding's part or receiver) | `open_whole13_rv36x_sp2`, `_rs1` | **fixed** 8ea94c78 (the tables stay keyed by name; an edge now reads any binding of the name) |
| OR-WHOLE-R37-DOC | S4 | documented over-refusals: a single-function binding from a call or reduce keeps its value half; three loop-fallback shapes | `rv37x_or_*`, `rv37x_overrefusal_loopfb_*` (wrong-class) | documented |
| TY-UNENFORCED (literal fields) | S1 | still open: a declared field type trusted for a struct literal (or list of them) holding another type | `open_rv37x_f_*` (8) | open |
| FV-OPEN-6 (container, coincidence) | S1 | still open: a function taken out of a container; the lane catches w1 only through a name coincidence | `open_rv37x_w1_nocoinc` | open |
| IFC-ALIAS-RESOLUTION (expression writes) | S1 | still open, ordinary lane: a function written in expression position in a callee, called with a secret field | `open_rv37x_ra_callee_pk`, `_stmt` | open (the ordinary unit) |

### Follow-up: 1696925b (fixes for the review of 9dc7be91)

Four fix designs and a cross-check for the 33 findings recorded at 28d21d21; merged and corrected as
the cross-check laid out. Matrix at 1696925b: 1364 cases, 1242 PASS, 34 silent accepts (all
`open_*`), 88 wrong-class.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| IFC-ALIAS-PREFERENCE | S1 | the two regressions of 9dc7be91 (Unknown identity set, flipped preferred alias) and two more the cross-check found (binders holding `abs`) | `open_ro1_alias_regression_*`, `ro2x_q_*_abs`, `rv36p_alias_reg_*` | **fixed** 1696925b: every resolvable candidate (FnMay), open by default |
| IFC-ALIAS-RESOLUTION | S1 | unresolvable branches, value-block tails, value-position function writes, lambda-and-name joins, call-result field types, a local named like a user function | `open_ro1_*` (8) | **fixed** 1696925b |
| IFC-SUMMARY-GAPS | S1 | the return summary and a local holding a function, expression-position push/insert, `?` as an exit (and under a secret condition), a field holding a function in a branch, a read after a value-block write, a lambda writing a captured name, secret keys in assignment targets | `open_ro1_ret_*`, `open_ro1_implicit_flow_*`, `open_ro1_enforcing_*`, `open_ro1_sym_*` (3) | **fixed** 1696925b |
| IFC-HOF-NAMED | S1 | named functions given to builtin and user HOFs, their capabilities, loop reassignment after the call | `open_ro1_hof_*`, `open_ro1_user_hof_*`, `open_ro1_loop_*`, `open_loop_reassign_after_use_secret` | **fixed** 1696925b |
| IFC-JOIN-BREAK-SHADOW | S1 | joins kept end states and skipped shadowing paths in the enforcing, value-block and parameter-return walkers | `open_rv36o_sib_*` (7), `rv37j_*` (26 leaks) | **fixed** 1696925b: ordered walks with break/continue exits and shadow keys |
| OR-ORDRET-CLOSURE | S3 | the order-free closure's over-refusals and per-level cost (9dc7be91) | `ro1_or1..3`, `rv36o_or_*` (4), review perf findings | **fixed** 1696925b |
| OR-SUMMARY-FNVALUE | S4 | documented over-refusal: in a summary a bare function name reads as its return, so a helper returning a secret-returning function is secret-returning (a list of such helpers, `len(map([1], getsec_v))`) | `rv21_valid_helper_value_unused_O1/O2/O5` (wrong-class) | documented |
| OR-OPEN-BINDERS | S4 | documented precision loss: a pattern, arm or loop binder over named functions is open (as before 9dc7be91) | `ro2x_q_for_twin`, `_twin1` (wrong-class) | documented |
| OR-ORDRET-OLD | S4 | older imprecisions the ordered walk no longer spreads: a declassify inside a helper, a field read on an unannotated let, field-insensitive root labels | `ro1_or4..6` (wrong-class) | open |
| TY-UNENFORCED (annotations) | S1 | a declared struct annotation trusted for a value of another type | `open_ro1_sym_mistyped_struct_annotation` | open |
| FV-CLOSURE-RETURNS-FN | S1 | a local closure returning a function | `open_ro1_closure_returning_function` | open |

### Follow-up: f878d41c (whole-struct lane, review round 38: four of five fix designs)

Review round 38 (pin whole14a) confirmed 46 findings: 35 leaks, 5 over-refusals, 6 costs. Five fix
designs and a cross-check; this commit lands push-loose, bands, boundary2 and domain2 part A (fnreturns
is the next unit). Matrix at f878d41c: 1565 cases, 1435 PASS, 35 silent accepts (all `open_*`), 95
wrong-class.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| WHOLE-PUSH-SHADOW | S1 | a `let push` anywhere hid a statement push from `written`/`changes`; the round-37 loose rules then dispatched a secret-holding struct as its element type (3 regressions of round 37) | `rv38_r38_loose_l1/l2/l4/l5_*`, `rv38_r38d_l5_*` | **fixed** f878d41c |
| WHOLE-SCALAR-TRUST | S1 | scalar_leaf and a let's declared struct annotation trusted types the runtime does not check | `rv38_r38_fix7_*`, `rv38_r38_declared_let_type_trusted` | **fixed** f878d41c |
| WHOLE-BANDS | S1 | round 37's phantom runs reached a function taken out of a container only two bands deep and only below a `let` (3 regressions of round 37); constructs re-run at the view lost guard writes and order | `rv38_r38b_l1/l2/l3/l9/l10/l11/l12_*`, `rv38c_u_mk`, `rv38c_u_main` | **fixed** f878d41c: `stick` / `exact_value`, constructs read as walked |
| WHOLE-JOIN-CAPTURES | S1 | statement joins and loop heads dropped what the lane found a binding may be as a function (3 regressions against whole10) | `rv38_r38b_1/2/3/4/5/6_*`, `rv38y_b2_*` | **fixed** f878d41c: join_captures, carried_captures, writes_seen |
| WHOLE-PART-WRITERS | S1 | a callee writing into a part of what it returns; self-part edges; Element roots in the strict parts query | `rv38_r38d_l1..l4_*`, `rv38x_domai_l4x_*` (6) | **fixed** f878d41c |
| OR-WHOLE-R37 | S3 | round-37 over-refusals and costs: twice rule, joined containers, phantom reads, bound-chain memory, nested wrappers | `rv38_r38d_o1/o2_*`, `rv38_r38b_o1/o2/12/p1..p3_*` | **fixed** f878d41c |
| PERF-NESTED-CALL-OPERANDS | S3 | this commit: bands reads a callback's returned functions through arg_local, which walks the operand again: 24 nested reduce seeds or 22 nested call/apply operands take exponential time | `rv30_valid_perf5_reduce_seed_nesting_exponential` (wrong-class at f878d41c) | fixed by the next unit (fnreturns, fns_at) |
| FV-OPEN-6 (round 38) | S1 | still open: a function taken out of a container, caught before only through a name coincidence | `open_rv38c_fut`, `_fut_nc`, `_u_mk_nocoinc`, `_u_main_nc` | open |
| OR-WHOLE-R38 | S4 | documented over-refusals: a loop variable over a list of functions is not followed; a lambda's write to a captured name is taken as reaching it; a sticky container rebind; an expression-position write keeping a stale alias (needs the ordinary lane); a block-local let under the twice rule | `rv38x_bound_acc_b15_*`, `rv38y_b2_v8`, `rv38y_b2_u4b`, `rv38x_bands_ovr_sticky_container_rebind`, `rv38_r38b_14_*`, `rv38x_domai_rv38d2_o1_or5_*` (wrong-class) | documented |
| PERF-R38B-13 | S3 | a nested capture chain costs 80-110 s on every pin (pre-existing) | review round 38 | open |

### Follow-up: 2f0b2805 (whole-struct lane, review round 38: fnreturns)

The fifth round-38 fix design, rebased onto bands, plus fns_at. Matrix at 2f0b2805: 1620 cases, 1492
PASS, 32 silent accepts (all `open_*`), 96 wrong-class.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| WHOLE-FN-RETURNS | S1 | a call's result was a plain value: a function a user function, method, closure, `call` / `apply` callback or `reduce` fold returns, a call through a value, `compose` (3 regressions against whole10) | `rv38_r38b_l4..l8_*`, `rv38_r38b_7..11_*`, `rv38x_fnret_*`, `open_whole2_B2`, `open_whole4_w11_ip_s06`, `open_whole7_r30_c04` | **fixed** 2f0b2805 |
| PERF-NESTED-CALL-OPERANDS | S3 | nested call / apply / reduce operands read again per level (introduced at f878d41c) | `rv30_valid_perf5_reduce_seed_nesting_exponential`, `rv38y_fr_nested_*` | **fixed** 2f0b2805: fns_at |
| OR-FN-RETURNS | S4 | documented over-refusals: compose applied to a whole struct; a recursive helper returning an ever-wrapped closure | `rv38x_fnret_documented_refusal_*` (2, wrong-class) | documented |
| IFC-CALLBACK-RETURN (ordinary) | S1 | still open, ordinary lane: `let g = call(|| pr); g(p.k)` (the whole-struct twin is fixed) | R38B-11 direct twin | open (the ordinary-lane unit) |
| FV-OPEN-6 (payloads, fields) | S1 | still open: a function taken out of a variant payload by a pattern binder, a container or a field | `open_whole4_w11_ip_s07`, `open_whole4_w11_ip_l11/l12` | open |

### Follow-up: cf7e812c (eighth review of the checker limits)

Twelve findings on whole15d (the pushed head's pin, which carries 2536d046), each re-run by a
verifier. None is an acceptance of a program that should fail; one was an rc-0 pass whose own
bundle contradicted it.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| CHK-RESERVE-STALE | S3 | 2536d046: the reserve was sized from the process's first request (FIRST_USABLE): a language server whose memory had since shrunk refused valid programs and exited with hundreds of MiB free | review E1, B8-1 | **fixed** cf7e812c: the reserve is an eighth of what is usable now |
| CHK-RESERVE-FLOOR | S4 | 2536d046: the reserve's fixed 128 MiB floor refused a program at 64% of a 280 MiB scope under ANUBIS_ANALYSIS_MEMORY_MIB | review B8-5 | **fixed** cf7e812c: the floor is at most a quarter of what is usable |
| CHK-EVIDENCE-LIMIT-VERDICT | S2 | the evidence lane's re-check or claim derivation stopping at a limit was recorded as a program FAIL: reported as ANUBIS_EVIDENCE_VERDICT_FAILED / repair_program, or under an rc-0 pass with a contradicting pca.json | review B8-2, B8-3 | **fixed** cf7e812c: the bundle records the limit (rejected claim, FAIL) and the command reports ANUBIS_ANALYSIS_LIMIT |
| CHK-PARTIAL-BUNDLE | S3 | an allocator exit inside the evidence lane left a bundle whose evidence and manifest said PASS, with no claim or hashes, while the exit text said nothing was written | review B8-4 | **fixed** cf7e812c: bundles are staged under a per-process `.partial` name and renamed when complete |
| CHK-VERIFY-INCONSISTENT | S3 | pre-existing: a forged PASS claim with `typecheck_ok: false` was "could not be re-derived" when the re-derivation stopped at a limit | review E3 | **fixed** cf7e812c: a claim no derivation produces is refuted before re-deriving |
| CHK-VERIFY-RESERVE-TEXT | S4 | 2536d046: verify blamed the budget and advised ANUBIS_ANALYSIS_MEMORY_MIB when the reserve had stopped it | review E5 | **fixed** cf7e812c |
| CHK-INDEPENDENT-FINDINGS | S4 | pre-existing: `?` outside a Result function and a constant return of another type were dropped after a limit | review E2 | **fixed** cf7e812c: push_diag_independent |
| CHK-ESCAPES | S3 | pre-existing: verify's signer line and evidence-verify's report printed a bundle's escape sequences; printable() let tag characters, line separators and fillers through | review E4, E6 | **fixed** cf7e812c |
| CHK-PARSE-COUNT | S4 | pre-existing: the JSON summary counted "… and N more parse errors" as one error | review E7 | **fixed** cf7e812c: `omitted` |

### Follow-up: e7b56507 (review of 1696925b: or-pattern arms and trailing push; first of the landing steps)

The review of 1696925b confirmed 59 findings (2 leak regressions, 34 older leaks, 17 over-refusals
and 6 costs of 1696925b). Six fix designs and a cross-check; this commit lands the two clusters that
hold the regressions. Matrix at e7b56507: 1761 cases, 1632 PASS, 32 silent accepts (all `open_*`),
97 wrong-class.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| IFC-OR-PATTERN | S1 | every lane bound all of an or-pattern arm's names for the whole arm, hiding the outer binding a non-binding alternative runs with (a regression of 1696925b in the return summary; older in the enforcing lane and the parameter summaries) | `ro3_reg_or_pattern_ret`, `ro3_or_pattern_enf`, `ro3_or_pattern_param`, `ro3x_orpat_*` | **fixed** e7b56507: one sub-arm per alternative in every lane |
| IFC-BINDER-SPAN | S1 | a binder with no span equalled an outer binding with none, so an all-binding match cleared the outer label | `ro3_enf_equal_span_binder_shadow` | **fixed** e7b56507 |
| IFC-TAIL-PUSH | S1 | a trailing `push(xs, v);` (the block's value at runtime) was never judged as the returned container (a regression of 1696925b in arm blocks; older elsewhere); a failed guard's push, a push nested in an expression, a loop header's sink before seeding | `ro3_reg_arm_tail_push_ret`, `ro3_tail_push_*`, `ro3_failed_guard_push_*`, `ro3_enf_*`, `ro3x_push_*` | **fixed** e7b56507 |
| OR-GUARD-BINDER-JOIN | S3 | 1696925b: a failed guard's join carried the arm's binders onto outer names of the same spelling | `ro3_or_failed_guard_binder_join` | **fixed** e7b56507 |
| PERF-NESTED-PARAM-FLOW | S3 | 1696925b: the parameter-flow closure walked statements nesting statements again per level (p5_scrut_10: 79.5 s) | `ro3y_perf_p5_scrut_10` | **fixed** e7b56507: shallow_param_flow (2.8 s) |
| OR-PUSH-NAMED-USER-FN | S4 | documented: a user function named `push` is judged as any user call, by its arguments (as every pin judges one of another name) | `ro3x_push_va2` (wrong-class) | documented |
| ORDRET3-REST | S1 | still open: the review's alias (15 leaks), summary (9 leaks), over-refusal (15) and cost (6) clusters | review of 1696925b | the next units |

### Follow-up: d61c33d8 (review of 1696925b, landing step 2: alias; a whole-lane cycle bounded)

The alias cluster of the review of 1696925b, the stack-overflow crash the alias candidate exposed
in the whole-struct lane, and a stronger walker-completeness gate. Matrix at d61c33d8 (pin
`anubis-ord3x2`): 1834 cases, 1695 PASS, 34 silent accepts (all `open_*`), 97 wrong-class, 8
INVALID (limit refusals, below).

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| ORD-ALIAS | S1 | function values written in branches, loops, value blocks and places, held in containers, forwarded by builtins or aliased to higher-order functions never reached their applications (16 findings + latent p23) | stage-B `ro3_*` / `ro3x_alias_*` cases (64) | **fixed** d61c33d8 |
| IFC-MAIN-BINDER | S1 | a main-scope binder keeping a closure (round 35; had no row) | `open_whole12_r35_r35i_2_l_mainbinder` | **fixed** d61c33d8 (closed by the alias join) |
| CHK-WHOLE-SCOPE-CYCLE | S2 | the whole-struct lane's resolve_local → Env::closure → widen_with → reach → resolve_local cycle had no guard (widening starts a fresh seen set); with alias's loop-written closures the checker overflowed its stack (`anubis check` too, on 64 MiB) | `limit_recursive_closure_source`, `rv30_valid_n03`; test `closure_analysis_limit` | **fixed** d61c33d8: Frame::enter on the scope path, cut() in reach |
| G19-SPAN-BORROW | S3 | a walker-completeness requirement could pass by matching a later sibling's call (`direct_local_consumer` did) | G19 poison tests | **fixed** d61c33d8: ScopedPattern (balanced-block scoping), per-consumer poisons |
| OR-ALIAS-SELFREF-LIMIT | S3 | precision regression of alias: a self-referential closure reassigned in a loop (`c = \|z\| c(z) + 1`) is refused with ANUBIS_ANALYSIS_LIMIT; at runtime the new closure calls the OLD snapshot, so the program is not recursive | `rv30_valid_n02`, `rv30_valid_n03` (INVALID) | **open**: closes when the lane models captures by value |
| FV-CAPTURE-BY-VALUE | S1 | the ordinary lane reads a closure's captures in the application scope, not by value at creation | `open_ord3_alias_defer_inner_capture`, `open_ord3_alias_defer_capture_by_value`, `open_ord3_alias_defer_shadowed_formal` | **open** |
| OR-ALIAS-PUSH-INDEX | S4 | an exact index read counts closures in weak push slots (the price of the pop-then-push fix) | `ord3_alias_defer_push_index_overrefusal` (wrong-class) | documented |
| OR-LOCAL-SHADOWS-FN | S4 | pre-existing (every pin): a local shadowing a printing user function is refused | `ord3_alias_defer_local_shadows_fn` (wrong-class) | documented |
| PERF-ALIAS-LOOP-LAMBDA | S3 | alias: loop-carried lambda chains are slower (lam_60_14 8.9 s → about 20 s; lam_120_10 42 → 47 s) | `ordret3fix/alias/perf/` | open |
| ORDRET3-REST | S1 | still open: the review's summary (9 leaks), over-refusal (15) and cost (6) clusters | review of 1696925b | the next units |

### Decisions under the owner's delegation (2026-09-25)

Recorded in [DELEGATED_DECISIONS_2026-09-25.md](DELEGATED_DECISIONS_2026-09-25.md), each reversible by
the owner:

| id | effect on this registry |
|---|---|
| IFC-PC-EGRESS | not a policy question any more: egress executed under a secret-decided condition is refused in Safe mode (a print under `if (k >> j) & 1 == 1` printed every bit of a secret on e7b56507); the row stays **open** until the fix lands in every lane |
| L-SHADOW-1 | lexical shadowing is the intended rule, reached through a diagnostic in the current edition and an edition change with a migration; open |

### Follow-up: d8a714f9 (whole-struct lane, review round 39, landing step 1: bands)

The review of f878d41c + 2f0b2805 (round 39) confirmed 33 findings: 19 leaks, 10 over-refusals and
4 costs, including a REGRESSION of round 38. Four fix designs and a cross-check; this commit lands
bands, which holds the regression. Matrix at d8a714f9 (pin `anubis-ord3x3`): 1942 cases, 1798 PASS,
38 silent accepts (all `open_*`), 98 wrong-class, 8 INVALID.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| WHOLE-R39-BANDS-REG | S1 | REGRESSION of round 38: `stick` kept a builtin name's implicit function only for egress sinks, so a `call`/`apply` rebound through a value block or pattern binder after container extraction lost the function it forwards (R39B-F1a/F1b; ordret2a rejects, ord3b accepts) | `rv39_bands_r39b_f1a`, `rv39_bands_r39b_f1b` | **fixed** d8a714f9 |
| WHOLE-R39-LAMBDA-STICK | S1 | a coincident lambda parameter (`\|show\| show`) lost the function a container held (R39B-F2) | `rv39_bands_r39b_f2`, `rv39_bands_t_rej_f2_lamparam_*` | **fixed** d8a714f9 |
| OR-WHOLE-R39-APPLY-POSITIONS | S4 | a nested literal `apply` confused operand positions and refused a valid program (R39B-O1) | `rv39_bands_r39b_o1`, `rv39_bands_r39b_o1b_k3_4levels` | **fixed** d8a714f9 |
| PERF-WHOLE-R39-BUILTIN-VALUES | S3 | needless callback analysis and repeated nested builtin application (R39B-P1, R39B-P2 7.57 s) | `rv39_bands_r39b_p1`, `rv39_bands_r39b_p2`, `rv39_bands_r39b_p1b_py_wide40`, `rv39_bands_t2_acc_p1_*` | **fixed** d8a714f9 (memoized) |
| OR-WHOLE-R39-HELD-POSITIONS | S4 | a non-literal list given to `apply` contributes the functions it holds at every position | `rv39_bands_t3_ovr_held_positions` (wrong-class) | documented |
| FV-OPEN-6-R39 | S1 | a main-scope builtin alias, a user-function formal, an ordinary list argument and a secret-field symmetric variant still leak (they depend on the ordinary lane's higher-order alias and formal handling) | `open_rv39_bands_t3_open_d2_main_call_let`, `…open_f2_formal_show`, `…open_nonlit_plain_arg`, `…sym_f2_pk` | **open** |
| WHOLE-R39-REST | S1 | round 39's boundary, fnret and loose clusters (and the cross-check's two unsound refinements, corrected by fix3) | review of f878d41c + 2f0b2805 | the next units |

### Follow-up: 892df460, 9d46b7e5, a05b6100 (whole-struct lane, round 39 steps 2-4: boundary, fnret, loose)

The remaining three fix designs of round 39, landed in the cross-check's order with its fix3
corrections for the two unsound refinements it found (U1 in fnret's reduce fold order, U2 in
loose's place-receiver arm). Matrix at a05b6100 (pin `anubis-ord3x4l`): 2157 cases, 1997 PASS, 39 silent accepts (all `open_*`), 111 wrong-class, 10 INVALID.

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| WHOLE-R39-BOUNDARY | S1 | break states dropped at the main-scope loop join (R39C-1); an if join lost a write made before a shadowing `let` (R39C-2); value-position blocks tracked no whole-struct assignment (R39C-3) and did no whole-lane join for their nested statements (R39C-4) | `rv39_boundary_*` | **fixed** 892df460 |
| OR-WHOLE-R39-SELF-READ | S3 | writes_seen distrusted the assigned name itself, refusing a self-read in a choice (R39C-6, R39P-O4, R39P-O5) | `rv39_boundary_*acc*` | **fixed** 892df460 |
| PERF-WHOLE-R39C-5 | S3 | the nested-loop closure-choice family re-interpreted per nesting level | `rv39_boundary_r39c_5_perf_nested_loop_closure_choices` | **reduced** 892df460 (about 4-5x the round-37 cost remains; documented by the design) |
| WHOLE-R39-FNRET | S1 | reduce chose its fold by is_callback (a closure-returning call seeded the wrong order; R39F-1); the accumulator lost a function seed or a function a fold returns (R39F-2..4); main-scope receivers trusted a callee's declared return, a declared container's element and a method's returned function (R39F-5..7) | `rv39_fnret_*` | **fixed** 9d46b7e5 |
| PERF-WHOLE-R39P-P1 | S3 | the cross-query memo dropped closure-returning specializations | `rv39_fnret_acc_p1_closure_rule_chain`, `rv39_fnret_acc_p1_p6_12_16` | **fixed** 9d46b7e5 |
| WHOLE-R39-FNRET-U1 | S1 | the design's reduce fold-order refinement accepted leaks the previous pins rejected (the round-39 cross-check's U1; f2_red_seed, f3, f5) | `rv39_xc_f2_red_seed`, `rv39_xc_f3_red_seed_var`, `rv39_xc_f5_red_main` | **fixed** 9d46b7e5 (fix3: drop the b-fold only when a is surely a closure; f6-f9 stay accepted) |
| WHOLE-R39-LOOSE | S1 | coarse part writers (R39-LOOSE-O1), a writer through a declared-result call or a method's arguments (LOOSE-1, -2), a writer handed to a builtin higher-order function (LOOSE-3), a let annotation over a builtin result, variant, literal, closure or other-typed place (LOOSE-4), a list rebuilt by `xs = xs + [u]` (LOOSE-5), `let s = s.step()` reading the new binding's type | `rv39_loose_*` | **fixed** a05b6100 |
| OR-WHOLE-R39-OWN-TYPES | S3 | methods on struct literals, own-type steps and returned method calls did not vouch for their type (R39P-O1, O2, O3) | `rv39_loose_*` | **fixed** a05b6100 |
| WHOLE-R39-LOOSE-U2 | S1 | loose's recv_type trusted a field's or position's declared type (TY-UNENFORCED): `w.t.next()`, `xs[0].next()` leaked (the cross-check's U2; l3-l6) | `rv39_xc_l3_place_recv`, `…l4_index_recv`, `…l5_callfield_recv`, `…l6_nested_chain` | **fixed** a05b6100 (the place arm removed) |
| OR-WHOLE-R39-RESIDUE | S4 | documented over-refusals of this stack: fnret's untyped-receiver residue (ov2_f, ov3_f, ov4_f, ov4_m, v10), loop-rewrapped closures (f9 loop_rewrap, pv1_f, pw13_m), loose's t09 (lost with the place arm), n5, u02, and the two twins fnret's receiver change refuses; the cross-check's nr_a1, nr_a3 | `rv39_fnret_acc_f8_*`, `rv39_fnret_acc_f9_*`, `rv39_loose_accept_*`, `rv39_loose_overrefusal_*`, `rv39_xc_nr_*` (wrong-class) | documented |
| OR-ALIAS-METHOD-RETURN | S3 | precision regression of the alias unit (d61c33d8), found while landing fnret: the ordinary lane takes a function a method returns from every impl of that name (`mkt().pick()(p)` calls T::pick, which prints a constant) | `rv39_fnret_acc_tw_f5_call_receiver_kept` (wrong-class; ord3b accepts it) | **open** |
| TY-UNENFORCED-R39 | S1 | a declared type the runtime does not check still lets a leak through: loose's claimed fix for a map under a struct annotation does not work (the runtime calls the map's closure under the method name), a one-`let` form of R39F-6, a variable of a place or index, a direct place receiver, a round-38 literal-field case | `open_rv39_loose_reject_r39_loose4_e2_map_under_annotation`, `open_rv39_xc_f6_onelet`, `open_rv39_xc_l9_var_of_place`, `open_rv39_xc_l10_var_of_index`, `open_rv39_xc_l3b_place_direct`, `open_rv39_loose_open_r38pl_dt_field_literal_field` | **open** |

### Reconciliation of the open items (2026-09-25; registered by d052f817)

Every open S1/S2 row here and every open "Known open issues" entry of CLAIMS.md was re-measured on
the head checker of that day (e7b56507); the report, its table and its probe programs are in
[docs/evidence/RECONCILE_2026-09-25/](../evidence/RECONCILE_2026-09-25/). Of 76 items: 42 open,
15 already fixed, 3 not reproducible as described, 16 non-code. The fixed rows below were stale;
their original rows are kept as they were written.

| id (row) | state now | evidence (measured on e7b56507) |
|---|---|---|
| F-PARSE-1, F-PARSE-2 | **fixed** (stale rows) | `parse_*` and `limit_nesting_*` cases: located ANUBIS_PARSE_ERROR, no abort |
| F-JSON-1 | **fixed** (stale row) | `--message-format json` reports verdict fail with a located parse error |
| D9 (residual) | **fixed** 1b40f653 (stale row) | `d9_unknown_arg_type` PASS |
| FV-OPEN-3 (residual) | **fixed** 2f0b2805 (stale row) | `open_whole2_B2` PASS |
| TY-SHADOW-RETYPE | **fixed** 8ea94c78 (stale row) | `open_whole13_rv36x_sp2_*`, `open_whole13_rv36x_rs1_*` PASS |
| IFC-ORDINARY-RETURN, IFC-JOIN-BREAK-SHADOW | **fixed** 9dc7be91, 1696925b (stale rows) | the rows' cases PASS (7/7 for the join-break shapes) |
| IFC-ALIAS-PREFERENCE, IFC-ALIAS-RESOLUTION, IFC-SUMMARY-GAPS, IFC-HOF-NAMED | **fixed** (stale open rows; the later 1696925b section records it) | the rows' `open_ro1_*` cases PASS |
| M-BUILTIN-HOF | **fixed** for apply/call/map over literals | `h_*_violated` refused, `h_*_satisfied` accepted; the residual is M-HOF-UNKNOWN |
| CLAIMS: secret-selected constants (nested) | **fixed** (stale heading; the body already says closed) | probes ssc1-ssc3 refused (registered by d052f817) |
| CLAIMS item 15 (research-lane gate immunity) | **fixed** (stale entry) | the gated-builtin predicate and its test exist |

Registered by d052f817 (49 REJECT cases, family RECONCILE-2026-09-25): 24 witnessed leaks, 22
check-only controls and container shapes, 3 with notes. 19 are silent accepts on the head checker
(open_rc25_*): the M-SINK-ARGS shapes (a printing builtin in a field, list or map, called with a
secret), M-JOIN-CLOSURE, IFC-LOOP-CARRIED-VALUEBLOCK, IFC-CALLBACK-RETURN (ordinary), and review
leaks of 1696925b's summary cluster.

Still open with a probe and no matrix case (not information flow; they need their own families):
| id | sev | defect | probe (evidence dir) | state |
|---|---|---|---|---|
| OBS-ASSERT-UNMODELED | S1 | an `assert` the checker cannot model is dropped silently: check passes and only the runtime trap catches it (contradicts SPEC.md's normative text) | `p/m/pj1`, `pj7`, `pj8`, `pj9` | **open** |
| M-IFLET-EXPR (expression if-let) | S1 | an `if let` used as an expression skips the contract check in its arm | `p/m/mi1`, `mi2`, `mi6` | **open** |
| M-GLOBAL-SCOPE (contract lane) | S1 | the contract lane resolves a local named like a global function to the global | `p/m/gs2` | **open** |
| M-HOF-UNKNOWN (reduce) | S1 | a contracted callback through `reduce` over unknown inputs is accepted | `p/m/mh3` | **open** |
| F-RESOLVE-1 | S1 | a module's struct with a secret field is overridden by a same-named public struct in main, and the secret prints | `p/fres2/` | **open** |
| TY-UNENFORCED (base) | S2 | a declared list type holding a string | `p/m/ty1` | **open** |
| F-SPAN-1 | S3 | semantic JSON diagnostics carry no location | `p/m/fs1` | **open** |
| A-EVID-1, A-EVID-2 | S2 | a bundle labels passing obligations "counterexample_replayed" and records the solver as z3 | the report's `pj1_evidence/` | **open** |
| A-SOLVER-1 (= CLAIMS item 6) | S2 | an obligation trusted to z3 with no certificate passes | `p/m/as2` | **open** |
| GEN-STRING-HEURISTIC | S2 | generics are a string heuristic: a long type-parameter name is wrongly refused, an Option annotation accepts a string | `p/m/gen1`, `gen2` | **open** |
| R-IFEXPR-FOLD | S3 | a struct-literal field in an if-expression initializer is wrongly refused | `p/m/rif1` | **open** |

### Follow-up: e516b1f3 (IFC v2: the information-flow interpreter, in every Safe-mode check)

Mandate section 6's semantic foundation lands: IFC v2 ([design](IFC_V2.md)), an abstract
interpreter that evaluates the program the way the runtime executes it, runs beside the other
lanes in every Safe-mode check (decision D8). Before landing, an independent adversarial review
(round 1: five lenses and a verifier) confirmed 107 findings against its first version (67
leaks, 31 over-refusals, 9 robustness); all are fixed and registered (family `IFC2-REVIEW-1`,
107 cases). Two of them were the runtime's own trap messages, fixed in the runtime (decision
D7). Decisions D6-D8 are in [DELEGATED_DECISIONS_2026-09-25.md](DELEGATED_DECISIONS_2026-09-25.md).

Measured before the round-1 cases were registered (2206 cases), IFC v2 alone against the lanes
(pin `anubis-ord3x4l`): it rejects all 58 registered leaks the lanes accept, accepts 105 valid
programs the lanes refuse, refuses no valid program the lanes accept, and reaches no limit; the
117 registered rejections it does not make are contract, capability and effect cases. Corpus
(968 programs) with IFC v2 beside the lanes: 0 verdict changes, certificates unchanged. Matrix at
e516b1f3 (pin `anubis-ifc2land-1`): 2319 cases, 2196 PASS, 0 silent accepts, 112 wrong-class, 11 INVALID (of the 113 cases added, 111 PASS; `ifc2r1_l20_duplicate_struct_literal_field` is refused as ANUBIS_DUPLICATE_FIELD before any flow check (INVALID) and `ifc2r1_or_12_max_nodes_fold_merges_record_fields` is refused by the lanes (wrong-class; IFC v2 alone accepts it); the 58 former silent accepts all PASS). Evidence:
[docs/evidence/IFC2_2026-09-26/](../evidence/IFC2_2026-09-26/).

| id | sev | defect | evidence | state |
|---|---|---|---|---|
| IFC-PC-EGRESS | S1 | an egress executed under a secret-decided condition (decision D1) | the D1 probes, `ifc2r1_ctl_*`, `ifc2r1_l06_*`, `ifc2r1_l12_*` | **fixed** e516b1f3 (IFC v2 refuses it in every Safe-mode check; the syntactic lanes alone still miss shapes of it) |
| FV-OPEN-6-R39 | S1 | the four ordinary-lane-dependent leaks of round 39 bands | `open_rv39_bands_t3_*` | **fixed** e516b1f3 (IFC v2) |
| TY-UNENFORCED-R39 | S1 | a declared type the runtime does not check lets a leak through | `open_rv39_loose_*`, `open_rv39_xc_f6_onelet`, `…l9_var_of_place`, `…l10_var_of_index`, `…l3b_place_direct` | **fixed** e516b1f3 (IFC v2 uses declared types only to add labels) |
| RECONCILE open leaks | S1 | the 19 open leaks registered by d052f817 (M-SINK-ARGS, M-JOIN-CLOSURE, IFC-LOOP-CARRIED-VALUEBLOCK, IFC-CALLBACK-RETURN on the ordinary lane, 1696925b's summary cluster) | `open_rc25_*` | **fixed** e516b1f3 (IFC v2) |
| earlier open leaks | S1 | the remaining registered open leaks of the value-join, whole-struct and ordinary-return families | `open_whole*`, `open_rv37x_*`, `open_rv38c_*`, `open_ord3_alias_defer_*`, `open_ro1_*`, `open_closure_returning_secret_fn`, `open_reentry_closure_capturing_secret`, `open_struct_field_reassigned_after_use_secret` | **fixed** e516b1f3 (IFC v2) |
| F-RESOLVE-1 | S1 | a module's struct with a secret field overridden by a same-named public struct in `main` | `docs/evidence/RECONCILE_2026-09-25/p/fres2/` (refused) | **fixed** e516b1f3 (IFC v2 gives a field every type either definition declares); no matrix case (the matrix runs single files) |
| RT-TRAP-OPERAND | S1 | the runtime's fail-closed traps printed their operands to stderr (index and length, missing key, counts, the unmatched value, file paths), so a secret index reached stderr | `ifc2r1_l17_*`, `ifc2r1_l21_*`; runtime test `runtime_traps_do_not_print_operand_values` | **fixed** e516b1f3 for the core runtime (decision D7); review round 2 reported more messages in the crypto and exploit-kit runtimes and the arity trap of a builtin used as a value: **open** |
| IFC2-REVIEW-1 | S1-S3 | the 107 findings of IFC v2's first review | `ifc2r1_*`; [findings table](../evidence/IFC2_2026-09-26/review1-findings.tsv) | **fixed** e516b1f3 |
| RT-TRAP-TYPENAME | S4 | a trap's message still names its operand's runtime type | decision D7 | documented (termination-channel family) |
| OR-LANES-UNION | S3 | IFC v2 accepts 105 registered valid programs the syntactic lanes refuse; the union still refuses them | `ifc2-alone-vs-lanes.tsv` in the evidence | **open** (retiring lanes IFC v2 subsumes is a later decision, on the matrix) |
| IFC2-REVIEW-2 | S1-S3 | review round 2 of IFC v2 (pin `anubis-ifc2-v8e`, the fixed version before its final landing changes): 75 findings, all confirmed by the independent verifier, none a repeat of round 1: 41 leaks (the lanes accept every one: existing silent accepts of the checker, not regressions), 28 over-refusals (22 of them valid programs the lanes accept, which IFC v2 in the union newly refuses; none is in the matrix or the corpus), 6 robustness | review report retained with the next unit, which registers every program | **open** (the next unit) |
| IFC2-STACK | S2 | found by the landing's own verification: IFC v2's recursion was not under the checker's stack guard, so 70 closures nested 60 lists deep on a 4 MiB stack overflowed (`closure_analysis_limit::deep_bodies_on_a_small_stack_refuse_instead_of_overflowing` aborted; the test run's summary counted no failure because the binary printed no result line) | `compiler/tests/closure_analysis_limit.rs` | **fixed** e516b1f3 (IFC v2 checks `analysis_limit::cut()` at every step; `ifc2-report` runs as a guarded request) |
| RT-LAMBDA-PARAM-SHADOWS-FN-VALUE | S2 | a user function named in value position (`[show, show]`) is lowered as a local when a lambda parameter anywhere in the same function has that name, so the native build fails (E0425) on a program `check` accepts | `docs/evidence/IFC2_2026-09-26/shadowfn.anb` (check rc 0; run: E0425); the same cause makes `rv39_xc_nr_a1_lamparam` and `rv39_xc_nr_a3_callb` unrunnable, so their ACCEPT intents have no runtime witness | **open** (the lowering's local set is flow-insensitive: `collect_local_names`) |
| IFC2-PRECISION-LAND | S3 | valid programs IFC v2 alone still refused after round 1: a group-by over computed map keys (an operand with no value gave a secret scalar), `false && …` and other constant conditions, `take`/`drop`/`chunk`/`window` with a literal count, a helper applied to 33 different callbacks past its context budget (one summary joined every callback) | `rv28_valid_or_b3_group_by_map`, `rv29_valid_or1_annotated_map_groupby_in_helper`, `rv33_valid_r33_o1_constant_short_circuit`, `rv29_valid_r29d_s04`, `rv33_valid_o01_nesting_builtins_lose_positions`, `rv32_valid_o6_33_closures_to_one_helper_shared_spec` | **fixed** e516b1f3 in IFC v2 (the lanes still refuse them: OR-LANES-UNION); `rv33_valid_r33_o3_continue_join_counter_reset` (a counter reset across a `continue`) stays refused (needs value tracking) |
| IFC-PC-EGRESS cases | — | the decision-D1 probes had no matrix case | `d1_pc_egress_*` (5 REJECT, 1 ACCEPT control; 5 with a runtime witness) | registered e516b1f3 |
