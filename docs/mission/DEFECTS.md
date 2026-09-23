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
| M-DIRECT-REQ (reproduced) | S1 | `fn g(x: i64) { f(x) }` with `f` requiring `x > 0`, no requires on `g`: `g(-1)` checks clean (g's params are unmodeled, the obligation is never emitted). Untyped variant (s02) is the unmodelable-clause drop | s01/s03, runtime | prototype measured, see mission log |
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
| M-DIRECT-REQ | S1 | an unmodelable `requires` on a DIRECT or method call is dropped with no obligation and no diagnostic: `discharge_call_requires` -> `carrier_unresolved_clause` returns early when `ctx.carrier_origin` is None (mod.rs:8340-8343), which holds for every non-carrier call; callee still assumes the requires, contracts not runtime-enforced (mod.rs:20082). Unmodelable `ensures` is refused, so this is asymmetric | mod.rs:8340,20082 | to-reproduce |
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
