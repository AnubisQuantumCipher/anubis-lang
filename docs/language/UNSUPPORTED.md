# Anubis Unsupported / PLANNED Surface (Gate 2/3 Truth)

**Rule:** Anything not implemented in this slice **must** be listed here and reflected in MATURITY_CLAIM_MATRIX.md + A_PLUS_ACCEPTANCE_CRITERIA.md. No over-claiming "general-purpose language complete".

## NOW REAL (previously planned — implemented 2026-07-09, see TURING_COMPLETENESS.md)

- `while` loops, `loop`, `break`, `continue`, and `for v in a..b` range loops — REAL and executed.
- Assignment / mutation (`x = expr;`) and indexed assignment (`a[i] = expr;`) — REAL.
- General recursion and mutual recursion (real call stack) — REAL.
- Arrays / lists: literals `[..]`, indexing `a[i]`, `len(a)`, `push(a, v)`, growable — REAL.
- Operators `/ % != && || !` and unary `-`/`!`, and `else if` chains — REAL.
- Integer digit separators (`1_000_000`) — REAL.
- **The executable language is Turing-complete** (loops + mutation + recursion), with a
  runnable Turing-machine witness. Evidence: `bash scripts/run_turing_core_fixtures.sh`.
- **Bounty-grade local PoC kit** — packing, `target_run`, process mutation fuzz, gold crash PoC.
  Evidence: `bash scripts/run_poc_kit_gate.sh`. See `docs/language/POC_KIT.md`.

## NOW REAL (runtime execution budget — fail-closed on runaway programs — 2026-07-11)

Because the executable language is Turing-complete, an `anubis run` program can loop
forever. It now runs under a **wall-clock budget** so a non-terminating (or hung)
program fails closed instead of blocking `anubis run` indefinitely and orphaning a
CPU-pinning child process.

- Default budget: **3600s** (the operator work-class-timeout invariant). On overrun the
  child is SIGKILLed and reaped; `anubis run` exits non-zero with `ANUBIS_RUN_TIMEOUT`.
- Override: `ANUBIS_RUN_TIMEOUT_SECS=<positive-int>` to change it, or `=0` to disable the
  cap (e.g. a deliberately long-lived interactive session).
- Scope note: only the direct run child is bounded. An `--allow-research` `target_run`
  probe already caps itself (2s) independently.
- Evidence: `cargo test -p anubis-compiler run_child_capped` (a real compiled spinning
  binary is killed inside the budget) + `run_timeout_policy` (default/opt-out parsing).

## NOW REAL (enums — 2026-07-09)

- `enum Name { Unit, Tuple(T, …) }` declarations — REAL
- Construction `Name::Variant` / `Name::Tuple(a, b)` — REAL
- `match scrutinee { Name::Variant => e, Name::Tuple(x) => e, _ => e }` expressions — REAL
- Executable via `anubis run`; proof-capable via RISC0 guest lowering
- Gate: `bash scripts/run_enum_match_gate.sh`

## NOW REAL (for-in collections — 2026-07-09)

- `for x in list { … }` — REAL (index walk over list/string via `len` + `index_get`)
- `for i in a..b { … }` — REAL (unchanged half-open range)
- Gate: `bash scripts/run_for_in_gate.sh` + turing fixture `for_in_list`

## NOW REAL (language power trio — 2026-07-09)

- **Maps / dictionaries** `{ k: v, … }` — REAL (`AnubisValue::Map`); index get/set `m[k]`;
  `len(m)`; `for k in m` iterates keys
- **Struct-like enum variants** `Err { code: u32 }` — REAL (decl, construct, match named bindings)
- **if-expressions** `let x = if c { a } else { b }` — REAL (`else` required; `else if` chains)
- Combined fixture: `examples/lang_power_trio.anb` → 42
- Proof fixture: `examples/proof/proof_lang_trio.anb` (if-expr + struct enum + named commits)
- Gate: `bash scripts/run_lang_trio_gate.sh`

## NOW REAL (A+ typing + match — 2026-07-09)

- Call-site type checks vs parameter annotations — REAL (`ANUBIS_TYPE_MISMATCH`, `ANUBIS_ARITY_MISMATCH`)
- Match exhaustiveness on known enums — REAL (`ANUBIS_MATCH_NON_EXHAUSTIVE`; `_` exhausts)
- Hex / binary / octal integer literals (`0x`/`0b`/`0o`) — REAL
- `target_run` returns named **TargetRun** struct (`r.crashed`, …) with list-index compat — REAL

## NOW REAL (multi-file modules — Phase-1, verified 2026-07-11)

- `import a.b;` resolves `src/a/b.anb` (or `a/b/mod.anb`) and `b::fn(x)` calls into it — REAL
  (`compiler/src/resolve/mod.rs`: module graph + file resolution). `pub` visibility enforced
  (`ANUBIS_PRIVATE_ITEM`); fail-closed on cycles (`ANUBIS_IMPORT_CYCLE`), path escape, ambiguity;
  the `A::B` enum-vs-module case disambiguates. Fixtures: `tests/fixtures/modules/{mathlib,
  private_reject,cycle,enum_vs_mod}`. (Supersedes the earlier "import parses but does not resolve"
  PLANNED note.) **`import std.*` is REAL (Phase 5)** — see NOW REAL Phase-5 section below.

## NOW REAL (float→integer narrowing rejection — Phase-2 slice 1, 2026-07-11)

- A **float value may not narrow into an integer annotation** — `let x: u32 = 3.14`, `-> u32`
  returning a float, or a float call-argument to an integer parameter — REAL (`ANUBIS_TYPE_MISMATCH`
  / `ANUBIS_RETURN_TYPE_MISMATCH`). Directional: integer→float **widening** (`let r: f64 = 3`) and
  all integer width-interop stay accepted. First rule to consume the structured `Ty` (new
  `ty::assignable` / `ty::is_float`); `ty::compatible` and its `ty_parity` frozen oracle are
  unchanged.
- Faithful to the runtime, not the annotation: bitwise/shift `& | ^ << >>` and unary `~` infer
  **integer** (they always return `Int` at runtime), so `let b: u32 = avg & 7` (float `avg`) is
  accepted; float arithmetic `+ - * / %` stays float. An `if`/`match` value is only "definitely
  float" when **every** statically-inferable branch is float (`if c { 3.14 } else { 5 }` is
  accepted — its taken branch may be the integer). These were adversary-found false positives,
  fixed before shipping.
- **Boundary (completeness gap, NOT a soundness hole):** narrowing fires only when the value's
  float type is statically inferable. A float arriving via a **function-return, an index/field
  access, or a block whose value is a trailing statement-form `if`/`match`** infers `None` and is
  NOT yet narrowed — it is accepted (the safe direction: a missed lint, never a false rejection,
  and the solver still fails closed on such a value with `ANUBIS_CONTRACT_UNPROVABLE`). Tests:
  `cargo test -p anubis-compiler float_does_not_narrow` / `narrowing_rule_does_not_reject`.
- **Why the call-return case is genuinely hard (finding, 2026-07-11):** a slice-2 attempt to close
  it by trusting a callee's *declared* `-> f64` return type was UNSOUND and reverted. A declared
  return type is not the runtime value type: the return check accepts int→float **widening**, so
  `fn g() -> f64 { return 5; }` actually returns `Int(5)` at runtime, and `let x: u32 = g()` runs
  fine (x = 5) — narrowing it on the declared `f64` wrongly rejects a running program. Closing this
  soundly needs per-function **return-value-class** summaries (does every `return` in the callee
  yield a float?), i.e. real interprocedural analysis (Phase 3), not declared types. Do not re-try
  the declared-type shortcut.

## NOW REAL (taint as a structured qualifier + index/field propagation — Phase-3 slice, 2026-07-11)

First step of Phase 3: taint recognition stops being a bare substring test and taint stops leaking
through indexing/field access.

- **Structured `tainted<T>` recognition.** `ty::is_tainted` (consumed by `is_tainted_type`, which
  seeds param/`let` taint) replaced the old `.contains("tainted")` substring check. That substring
  version false-positived on any type merely NAMED with the substring — a struct called
  `TaintedRecord` was wrongly seeded tainted. The anchored `.contains("tainted<")` fixes that AND
  correctly catches a qualifier nested in a container (`list<tainted<u32>>`, `Option<tainted<u32>>`,
  `Map<string, tainted<u32>>`), which the truly-anchored whole-string guard would have MISSED — an
  adversarial round caught that as a real security regression before it shipped.
- **Index / field-access no longer launders taint.** `expr_taint_source` gained `Expr::Index` (checks
  both base and index) and `Expr::FieldAccess` (base) arms. Previously `sink(tainted_arr[i])` and
  `sink(tainted_struct.field)` fell through a catch-all and silently escaped
  `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` — a real fail-open, now closed.
- **Interprocedural return-taint + cast propagation (Phase-3 slice 2).** `sink(get_secret())` where
  `get_secret` returns an internally-produced taint (a `taint_source()`/`tainted<T>` local, through
  let-chains, casts, and transitive returns of other tainting functions) is now flagged, via a
  monotone fixpoint summary consulted by `expr_taint_source`'s `Call` arm. See the boundaries below.
- Tests: `is_tainted_*` (ty.rs, incl. the frozen-oracle-adjacent VOCAB test),
  `taint_propagates_through_field_access_and_indexing_to_sink`,
  `is_tainted_detects_qualifier_nested_in_a_container_annotation`,
  `taint_is_flow_sensitive_across_reassignment_residual_closed`,
  `interprocedural_return_taint_is_flagged_at_the_call_site`.

**Closed / current boundaries:**
- **Taint flow is now reassignment-SENSITIVE (residual CLOSED — Phase-2 taint soundness).** The
  former fail-open — a clean var reassigned to a tainted value was NOT re-tainted, so `let x = 5;
  x = input(); sink(x)` compiled clean — is closed: the `Assign` handler propagates the RHS's taint
  to the binding (SET on tainted, CLEAR on clean/declassified), and `if`/loop bodies MERGE taint
  may-taint (`merge_taint_over`) with binding-identity by span so a block-local `let` SHADOW never
  leaks to the outer binding. Both directions are precise: reassign tainted→clean clears; reassign
  clean→tainted taints; a value tainted on any path stays tainted (fail-closed); cleared on every
  path clears. This is the proper control-flow-merge dataflow the three adversarial rounds required
  (the shadow-vs-reassign case they flagged is handled by the span identity check). Fixtures:
  `taint_reassign_straightline`/`_branch`/`_to_clean_accepts`; tests: `taint_reassignment_*`,
  `taint_is_flow_sensitive_across_reassignment_residual_closed`, `block_scoped_shadowing_*`.
- **CONFIDENTIALITY flow is REAL (Phase-2 leg-1, `c727b8d`) — with named boundaries.** The dual of
  the taint integrity flow: a value seeded by `secret_source(..)` that actually REACHES a
  network/shell egress (`send`/`network_send`/`connect`/`http_get`/`http_post`/`shell`/`exec`/
  `system`/`target_run`, exact-match) without a well-formed `declassify(value, policy, reason)` is
  `ANUBIS_SECRET_EXFILTRATION` (Safe mode, enforcing). The `secret` label lives on `ScopeBinding`
  (analysis-only, never serialized), flows through let/assign (SET on secret RHS, CLEAR on clean),
  and merges may-secret across branches/loops via the same `merge_taint_over` + span-identity as
  taint. A malformed declassify does NOT release (AST-shape keyed). Boundaries this slice, stated
  not hidden: **no interprocedural secret summary** (a secret returned from a helper arrives
  unlabelled — the dual of `compute_tainting_fns` is future work); **`getenv`/params are not
  auto-labelled** (needs a `secret<T>` qualifier decision); **local `fs.write` is not egress**
  (a secret written to a local file passes — pinned by `secret_local_write_accepts.anb`). Fixtures:
  `secret_exfiltration_send`/`_reassign`/`_shell`, `secret_declassified_egress_accepts`,
  `secret_local_write_accepts`, `secret_reassigned_clean_before_egress_accepts`; tests:
  `secret_flows_to_egress_without_declassify_rejects`, `secret_egress_accept_edges_are_precise`,
  `secret_egress_malformed_declassify_still_rejects`.
- **CONFIDENTIALITY + leg-2 are now INTERPROCEDURAL (Phase-2, `8624882`).** Two monotone-fixpoint
  summaries, siblings of `compute_tainting_fns`: `secret_fns` (`compute_secret_fns` — functions whose
  return carries a secret; consumed by `expr_secret_source`'s Call arm AND the trifecta leg-1) closes
  the "secret returned from a helper" boundary (`send(get_key())` now fires); `leg2_fns`
  (`trifecta::compute_leg2_fns` — functions whose body PRESENCE-exposes untrusted input, built with
  `is_leg2_source` so file reads are excluded) makes the trifecta leg-2 interprocedural (a helper
  wrapping `input()` is untrusted-input exposure). Discard-arg precision is preserved via the shared
  `param_return_taint` summary (`send(ignore(secret))` with `fn ignore(x){0}` does NOT fire); a
  well-formed declassify inside a helper is a release barrier ACROSS the call boundary (a sanitizing
  helper is not a leg-2 exposer). Fixtures: `secret_exfiltration_via_helper`,
  `secret_discard_helper_accepts`, `lethal_trifecta_interproc_helpers_verified`.
- **The LETHAL TRIFECTA now runs in the Safe (default) lane, SHADOW-FIRST (Phase-2, `ec69ab6`).** The
  coexistence check (one function holding private-data access + a distinct untrusted-input channel + a
  net/shell egress, with no well-formed declassify) is ENFORCING in the verified lane and SHADOW-gated
  in Safe (`emit(.., !ctx.verified)`): under a normal `check` a 3-leg body still compiles (default-lane
  verdicts unchanged), and under `ANUBIS_SHADOW_TYPES=1` it emits `ANUBIS_LETHAL_TRIFECTA` for the
  corpus shadow diff (which fires on nothing committed → `UNEXPECTED=0`). The accept-bias of a
  coexistence check in the default lane was decided with the operator: land shadow-first, PROMOTE to
  Safe-enforcing (a single-flag flip) after it soaks against real programs. Residuals: `http_get`/
  `http_post` are not yet a net.send effect so leg-3 under-fires an http constant-beacon; in Safe an
  open effect row stays legal so leg detection under-approximates (accept-biased); the declassify hatch
  is coarse (function-level); leg-isolation only relieves when the untrusted-input path and the private
  read+egress share no common transitive caller (a lone `main` re-forms the trifecta).
  **Remaining named boundaries (stated, not hidden — adversarial-review-confirmed):**
  - **Composite laundering** (symmetric with taint, pre-existing): `expr_secret_source`/
    `expr_taint_source` have no `ArrayLiteral`/`StructLiteral`/`MapLiteral`/`EnumConstruct` read arm,
    and `expr_param_return_flow` (which builds the shared `param_return_taint`) returns the empty set
    for those + `Match`/block-`If`. So a secret stashed into a container/struct, or a pass-through
    helper that wraps its arg in one before returning, is not tracked. Fixing `expr_param_return_flow`
    improves BOTH labels — a high-leverage follow-up slice, kept separate because it changes taint-side
    behavior and needs its own corpus re-validation.
  - **Confidentiality param→egress-sink dual is REAL (Phase-2, `3d94861`).** The interprocedural twin
    of `ANUBIS_SECRET_EXFILTRATION`, exactly as `ANUBIS_INTERPROC_SINK` is the twin of the direct
    tainted-sink check. `compute_param_egress` (a monotone fixpoint sharing the `param_sinks` value-flow
    walk, parameterized by the leaf-sink predicate — `is_egress_sink` here vs `is_sink` for integrity)
    summarizes which formals reach a net/shell EGRESS; a SECRET argument in such a position is
    `ANUBIS_INTERPROC_EXFILTRATION` at the call boundary, so `leak(secret_source())` with
    `fn leak(x){ send(x) }` (or `shell(x)`, or a transitive `a→b→send`) is caught. EGRESS-only, so a
    secret into a LOCAL write is not flagged; a declassified arg releases; a discard helper accepts; a
    TAINT arg fires the integrity `ANUBIS_INTERPROC_SINK` instead (orthogonal labels). Validated by a
    full-corpus verdict-diff (shadow-diff is blind to the enforcing emit). Residual: the summary reuses
    `expr_param_flow`, which is not `param_return`-aware, so a secret flowing ONLY into a discarding
    forwarding-call is over-flagged — exact parity with the pre-existing `param_sinks` over-approximation,
    corpus-inert; tightening it (make `expr_param_flow` consult `param_return_taint`) is a separate change
    that would improve BOTH labels. Fixtures: `secret_into_egressing_helper_rejects`,
    `secret_into_shell_helper_rejects`, `secret_into_egress_transitive_rejects`,
    `secret_egress_declassified_arg_accepts`, `secret_into_local_write_param_accepts`,
    `secret_into_discard_helper_accepts`.
  - **No `secret<T>` qualifier**: `getenv`/param secrets are not auto-labelled (a surface-syntax
    decision, mirroring `tainted<T>`).
  - **Presence-level declassify hatch** (pre-existing): any one well-formed declassify in an agent body
    suppresses the trifecta, even if applied to unrelated data — the hatch is not tied to the outbound
    value. Interprocedural legs make it slightly easier to trip.
  - **Callee-name keyed**: method/closure-valued calls (`recv.m()`, `let f = get; f()`) are not
    resolved by the interprocedural summaries (same boundary as the taint side).
  - ~~**Tail if/match implicit return**~~ **RETIRED (`fe44f35`)**: a secret/taint returned via a bare
    tail `if`/`match`/`if let`/block is now summarized — the scope-aware walkers walk control-flow tails.
- **COMPOSITE / aggregate flow is REAL (Phase-2, `a930e7e`) — on both labels, intra + interproc.** A
  tainted/secret value nested in a container literal no longer launders: the pure-aggregate arms
  (`ArrayLiteral`/`StructLiteral`/`EnumConstruct`/`MapLiteral`/`Try`) were added to all four flow
  walkers — `expr_taint_source`, `expr_secret_source`, `expr_param_return_flow`, `expr_param_flow` —
  so `sink([tainted])` / `send([secret])` / `send(Struct{f: secret})` are caught, and a pass-through
  helper `fn wrap(x){ return [x]; }` summarizes `{0}` (caught across the call boundary). Any
  sub-expression carrying a label makes the aggregate carry it; a well-formed declassify inside a
  container still releases. Fixtures: `taint_in_container_to_sink`, `secret_in_container_to_egress`,
  `secret_container_passthrough_helper`, `clean_container_to_sink_accepts`.
  **Follow-on now landed:**
  - **CONTROL-FLOW value expressions are REAL (Phase-2, `fe44f35`) — SCOPE-AWARE, on both labels,
    intra + interproc.** All four flow walkers gained `Match`/`If`/`IfLet`/block arms that build a
    LOCAL extended scope (clone the ambient scope/flow, seed the new bindings so they SHADOW outer
    same-named ones, recurse into the value in the clone). This closes the composite follow-on both
    ways: an inner binding (pattern var / block-local `let`) shadowing an outer tainted/secret binding
    is the arm's own clean binding (no false positive — the exact defect the review reverted from the
    composite slice), and a value passed THROUGH an inner binding is tracked (block-local `let`,
    match/if-let pattern destructure of a secret/tainted scrutinee, if-expression branch — no false
    negative). A straight-line `Assign` to a var inside a value block applies the main analyzer's
    set/CLEAR discipline to the clone, so reassign-to-clean in a value branch stays accepted. Pattern
    vars inherit the WHOLE scrutinee's label (whole-value granularity, matching `Index`/`FieldAccess`).
    Interprocedurally, a param destructured through a match arm flows to the return summary
    (`fn pick(x){ return match x { _ => x }; }` summarizes `{0}`). The old **tail `if`/`match` implicit
    return** boundary (integrity AND secrecy) is retired — the return summaries feed the tail expr to
    the now-scope-aware walkers. Design + landed code both adversarially reviewed (read-only). Fixtures:
    `secret_match_destructure_to_egress`, `secret_if_branch_to_egress`,
    `taint_block_let_passthrough_to_sink`, `secret_interproc_match_passthrough`,
    `control_flow_shadow_pattern_var_accepts`, `taint_reassign_to_clean_in_branch_accepts`,
    `clean_match_value_to_sink_accepts`, `declassify_in_branch_releases_accepts`.
  **Follow-on now landed:**
  - **Sink/egress/capability CALLS buried inside a control-flow value expr are REAL (Phase-2,
    `984ff80`) — SCOPE-AWARE, enforcing.** The two effect/sink-detection passes now descend into
    `Match`/`If`/`IfLet`/block: `analyze_expr_effect` (the enforcing pass) gains scope-aware arms
    (clone-per-arm/branch/block, `seed_effect_pattern`/`seed_effect_let` carrying BOTH labels so a
    buried sink resolves the inner shadowed binding; conditions and match guards walked for effects),
    and `collect_param_sinks_in_expr` (the param→sink summary) gains the mirrored arms so a param
    reaching a sink through a match arm is summarized. This closes a Safe-mode **capability-laundering
    bypass**: `if true { shell("id") }` / `if true { send(x) }` with no `uses(...)` clause was accepted
    (the effect was never registered); it is now `ANUBIS_EFFECT_FORBIDDEN_IN_MODE`. Also enforced:
    tainted-sink / secret-egress / `ANUBIS_INTERPROC_SINK` buried in an arm/branch/block. Accept-bias
    holds — declassify-in-branch releases, a declared capability accepts, reassign-to-clean clears, a
    pattern var shadowing an outer secret is the arm's own clean binding, a buried `read_file` stays
    Safe-allowed. Validated by a full-corpus BEFORE/AFTER verdict-diff (139/66 unchanged, zero flips) —
    the correct gate, because these enforcing diagnostics are invisible to the shadow-diff harness.
    Design + landed code both adversarially reviewed. Fixtures: `buried_sink_in_if_branch_flagged`,
    `buried_sink_in_match_arm_flagged`, `buried_secret_egress_in_block_flagged`,
    `buried_capability_launder_forbidden`, `buried_shell_launder_forbidden`,
    `buried_interproc_sink_through_match`, `buried_declassify_in_branch_accepts`,
    `buried_reassign_clean_before_sink_accepts`, `buried_clean_call_with_capability_accepts`.
  **Named boundaries (adversarial-review-confirmed, still deferred):**
  - **Calls buried in NON-control-flow compound exprs** are still not walked by `analyze_expr_effect`
    (it recurses `Call` args, `== / !=` `Binary`, and the new control-flow arms only): a sink/privileged
    call used as an aggregate element (`send([leak()])`), an index (`arr[sink()]`), a cast/unary
    operand, a non-`==`/`!=` binary operand (`send(x) + 1`, and therefore a comparison-wrapped match
    guard `_ if send(x) > 0`), or a `?`-operand is not enforced. A uniform-recursion follow-up.
  - **Nested statement control-flow inside a value block** merges with snapshot/restore only (no
    cross-body taint merge), so a loop-carried label escaping to the block tail is fail-open (matches
    the value-walker's block handling); the param→sink summary's block arm does not recurse nested
    control-flow statements (a monotone, fail-open under-approximation — never a false interproc
    reject). Non-`Var` assign targets, `Assume`-inner, and research/exploit/spec blocks in a value
    block are left to their existing handling. `check_expr_semantics` has no `IfLet` arm, so a call
    buried in an `if let` gets its first arity/type check from the effect pass (a correct new reject).
  - **Closure application** (`CallExpr`: `f(a)(b)`, `arr[i](x)`, `recv.m(x)`) and the **`Lambda`
    literal** body (captures/params unmodeled): a secret/tainted value through a closure is not walked
    (higher-order — the last binding-introducing `Expr` shapes left at the catch-all).
- **Interprocedural RETURN-taint is now modeled (Phase-3 slice 2).** A monotone fixpoint pre-pass
  (`compute_tainting_fns`, run before per-function analysis) marks each function whose return value
  carries INTERNAL taint — a `taint_source()`/`tainted<T>` local returned directly (through let-chains
  and casts), or a return of another marked function. `expr_taint_source`'s `Call` arm consults it, so
  `sink(get_secret())` is now flagged even with no tainted argument. The return-taint walk is
  scope-aware (respects lexical block shadowing) and declassify-aware (a function that declassifies
  before returning is clean). Also this slice: taint now propagates through an `as` cast (a new
  `Expr::Cast` arm — `sink(s as u64)` no longer launders).
  **parameter→sink summaries are REAL (Phase-3 A1):** monotone `compute_param_sinks` + call-site
  `ANUBIS_INTERPROC_SINK` so `fn log(x){sink(x);}` makes `log(tainted)` a violation at the call site.
  **argument→return pass-through summaries are REAL (Phase-3 A2):** monotone `compute_param_return_taint`
  marks which formals flow to the return; `expr_taint_source`'s Call arm consults it so
  `fn wrap(x){return x;}` makes `wrap(tainted)` (and chains like `f→wrap→return`) taint their result.
  Known user functions that do NOT return a param no longer over-taint (`ignore(secret)` is clean).
  Still NOT modeled interprocedurally: **higher-order / indirect calls** (`let f = get_secret; sink(f())`
  — the summary keys on the callee NAME; a function-valued variable is not resolved, same boundary as
  method calls via `CallExpr`).
- **Block-scoped shadowing is now respected by both the return-taint summary AND the intra-procedural
  sink check (Phase-3 slice B, 2026-07-11).** `analyze_stmts` snapshots/restores the lexical binding
  scope around `if`/`else`/loop/`@research`/`@exploit`/hybrid bodies the same way `body_returns_taint`
  already did — so `let x=5; if c { let x=taint(); } sink(x);` correctly accepts the outer clean `x`
  (the previous fail-CLOSED false positive is closed). Solver assumptions/`solver_int_vars` stay on
  their own snapshot path and are not disturbed by the taint-scope restore. A real sink of the *inner*
  shadow, or of an outer binding that is itself tainted, is still rejected.
- **Whole-binding granularity.** A struct field individually declared `tainted<T>` in the struct's
  own type definition does not by itself taint `.field` access on an otherwise-clean instance; only a
  binding seeded tainted at its own `let`/param propagates.
- **Outermost-and-nested, but not every position.** `is_tainted` matches `tainted<` anywhere in the
  annotation string (so container-nested qualifiers are caught); the tradeoff is a hypothetical
  future generic type whose own name ends in "…tainted" immediately before its bracket would be
  over-flagged — the SAFE direction for a security check, and no such type exists in the corpus.

## NOW REAL (shipped after this slice — verified firsthand 2026-07-10)

These were listed as PLANNED below in an earlier slice and have since shipped. Moved up so the ledger
is honest in **both** directions (never over-claim, never under-claim):

- **Traits, `impl` blocks, and (erased) generics** — REAL and golden-tested
  (`generic_syntax_is_accepted_and_erased`, `generic_trait_with_nested_params`,
  `traits_default_methods_and_overrides`, `inherent_method_beats_trait_default`). Default methods,
  overrides, and inherent-beats-trait resolution all work. Generics parse and erase.
- **Built-in `Option` / `Result` + the `?` operator** — REAL. `Some`/`None`/`Ok`/`Err` need no decl;
  `?` short-circuits on `None`/`Err` (verified: an `Err` propagates out through `?`).
- **Block comments `/* ... */`** — REAL and nesting-aware (`frontend/mod.rs`).
- **Full string / list / map library** — REAL: ~150 builtins including
  `upper`/`lower`/`trim`/`split`/`contains`/`starts_with`/`ends_with`/`replace`/`index_of`/`repeat`/
  `substr`/`char_at` (strings); `map`/`filter`/`reduce`/`sort`/`zip`/`enumerate`/`flatten`/`any`/`all`
  (lists); `keys`/`values`/`entries`/`has_key`/`get`/`merge` (maps). See `docs/language/STDLIB_CORE.md`.
- **Fail-closed indexing** — `xs[i]` / `m[k]` trap on out-of-bounds / missing key (2026-07-10);
  `get(coll, key, default)` / `has_key` are the optional-access path.
- **`input()` / `read_line()`** — REAL (stdin forwarded to the run binary, 2026-07-10).

## NOW REAL (effect clause `uses(...)` — Phase-3 C1+C2, 2026-07-11)

- Parse `fn f(...) uses(fs.read, net.send, time.now, rand.gen) { ... }` on `Item::Fn.effects`.
- Declared-vs-inferred: when `uses` is present, every capability effect inferred from the body
  (`file_read`/`file_write`/`network`/`shell`/…) must be ⊆ the declared set →
  `ANUBIS_UNDECLARED_EFFECT`. Absent `uses` skips the check in the default lane.
- Effect inference now sees calls in let-initializers and nested call arguments (not only bare
  expression statements).

## NOW REAL (I/O ↔ taint + verified lane — Phase-3 C4+C5, 2026-07-11)

- **I/O reads are taint sources:** `read_file` / `open` / `input` / `read_line` seed taint via
  `expr_taint_source` (`io source \`...\``).
- **I/O writes/sends are sinks:** `write_file` / `write` / `send` / `network_send` are in `is_sink`,
  so undeclassified read→write/send is `ANUBIS_TAINTED_SINK_WITHOUT_DECLASSIFY` (same machinery as
  `sink(...)`).
- **Declared `uses` authorizes Safe-mode I/O (dual-mode crown):** `uses(fs.write)` allows
  `write_file` in Safe; `uses(net.send)` allows `send`/`connect`; without the matching uses,
  Safe still fails with `ANUBIS_EFFECT_FORBIDDEN_IN_MODE`. (Earlier gap: hard forbids ignored uses.)
- **Verification lane:** `typecheck_ex(..., verified=true)`, CLI `--verified`, and item attrs
  `@verified` / `#[verified]`. Capability effects without a `uses(...)` clause fail closed
  (`ANUBIS_UNDECLARED_EFFECT`). Default lane remains permissive when `uses` is absent (for
  `fs.read`; write/net still need uses or research).
- **Multi-file:** `anubis check` resolves `import` graphs like `run` (`combine_from_entry`). Fixture:
  `tests/fixtures/modules/phase3_io/` (clean path + leak path).

## DEFERRED (Phase-3 A3 — field/element-granular taint)

- **Whole-binding granularity remains.** A struct with one `tainted<T>` field still does not seed
  only `.that_field` as tainted on an otherwise-clean instance; taint is per binding (let/param),
  not per field/element of a clean binding. This is a **precision** gain (fail-closed over-approx
  when a whole binding is seeded tainted), not a fail-open. Deferred: needs field-sensitive
  BindingInfo / path-qualified taint labels and sound joins at merges — not shipped half-working.

## Contract composition (Phase-3 D — status)

- **Present:** callee `requires`/`ensures` are registered in `fn_contracts` and at `let x = f(args)`
  specialized: preconditions become solver obligations (`requires@callee:...`); postconditions are
  assumed into the caller only when every requires was modelable and the callee returns an integer.
- **Not a separate error code:** unmet requires surface as failed solver obligations / check failure,
  not a dedicated `ANUBIS_CALLEE_REQUIRES_UNMET` diagnostic. Cross-file composition works through the
  resolved flat namespace (same as other interprocedural summaries). No extra slice needed for the
  core path; a named error code remains optional polish.

## NOW REAL (Phase-4 — proof-surface breadth + verifier trust, 2026-07-12)

Deepens the solver moat. Everything below is fail-closed and unit-tested (`cargo test -p anubis-compiler --lib phase4_`).

### A1 — Safe division / modulo / shift (`bvashr` lock)

- `/` and `%` model as `bvsdiv`/`bvsrem` only when the divisor is a non-zero literal **or** a
  parameter proven non-zero by `requires(d != 0)` / `requires(d > 0)` (and never reassigned) —
  marked via `nzdiv` side-channel in `solver_int_vars`.
- Unguarded or zero divisor → **`ANUBIS_DIVISOR_MAYBE_ZERO`** (not a silent cert). Modeling an
  unguarded divide would ignore the runtime trap.
- `>>` is **arithmetic** (`bvashr` + mod-64 mask), matching `i64::wrapping_shr`. `bvlshr` would be
  unsound (the historical 32-bit-unsound bug class). Locked by discharge tests (`-8 >> 1 == -4`).

### B1 — Real counterexample replay

- Every z3 `sat` FAIL path runs `replay_counterexample`: model-substitution + independent
  re-`check-sat`. A model that does not re-verify is **`ANUBIS_REPLAY_MISMATCH`** (encoder-vs-solver
  soundness alarm) — fail closed.
- Ground formulas (no free constants) legitimately return an empty model `()`; those re-check the
  base query alone. Open formulas with unparseable models still fail closed.
- Evidence SARIF maps `ANUBIS_REPLAY_MISMATCH` (legacy alias retained).

### B3 — Program-level differential harness (`middle/proptest.rs`)

- Deterministic LCG generates random modelable pure-i64 programs (true contracts + false contracts).
- Property **P_discharge:** solver says discharged ⇒ runtime prints the oracle value.
- Property **P_disproof:** solver says FAIL ⇒ model (when present) replays; runtime body value is
  the true result (not the wrong ensures constant).
- Standing regression net against false-proofs for every future proof-surface change.

### A2 — Bounded arrays / sequences (QF_ABV)

- List **literals** of int-modelable elements (and lets bound to them) model as SMT
  `(Array (_ BitVec 64) (_ BitVec 64))` + length BitVec; obligations use **`QF_ABV`** when arrays
  appear, else `QF_BV`.
- `len(xs)` and `xs[i]` are int-modelable only for bounded modeled sequences with a **proven
  in-range non-negative constant index** (negative indices exist at runtime but are not modeled).
- Index not proven in-range → **`ANUBIS_INDEX_MAYBE_OOB`**.
- Unbounded lists (parameters, push results, …) → **`ANUBIS_SEQ_UNBOUNDED`**.
- Tests: `phase4_bounded_seq_qf_abv_and_fail_closed_codes`.

### S — Strings / floats (opaque, precise diagnostics)

- String contracts → **`ANUBIS_STRING_CONTRACT_UNMODELED`**.
- Float contracts → **`ANUBIS_FLOAT_CONTRACT_UNMODELED`**.
- Optional QF_S / QF_FP lanes remain **PLANNED** behind feature flags + the same fail-closed
  discipline (not shipped). Tests: `phase4_string_and_float_opaque_diagnostics`.

## NOW REAL (Phase-5 — Anubis-source standard library, 2026-07-12)

- **Embedded `import std.*`** — modules under `compiler/stdlib/std/*.anb` baked via `include_str!`
  (`compiler/src/stdlib/mod.rs`). `std.*` resolves **only** from the embedded registry (no project
  shadowing). Content lock: `compiler/stdlib/MANIFEST.sha256`.
- Modules (**10**): `std.collections` (Set + OrderedMap), `std.iter`, `std.option`, `std.result`,
  `std.str`, `std.math` (`math_add`/`math_sub`/… with contracts — not bare `add`), `std.testing`,
  `std.io` (uses + taint), `std.pwn` (pack LE/BE, unpack, cyclic_find, pad/junk/chain, crash helpers,
  run_local), **`std.crypto`** (HMAC CT verify, HKDF, CSPRNG, ChaCha20-Poly1305, Argon2id/PBKDF2
  password_hash, PHC, Ed25519 — see `CRYPTO.md`).
- Rust builtins remain the microarchitecture; stdlib is composition. Phase-2/3 summaries apply after
  combine (namespaced `std_io__read_text`, etc.).
- **Capability inheritance (fail-closed):** a caller's Safe/verified gates inherit the callee's
  declared `uses(...)`. `std.io::write_text` / `std.pwn::run_local` cannot launder `fs.write` /
  `shell` past a caller that omitted the capability (`ANUBIS_EFFECT_FORBIDDEN_IN_MODE`).
- **std.pwn**: packing always available; `target_run` / `run_local` requires `--allow-research`
  (runtime panic if called without). Gold: `examples/security/poc_stdlib_overflow.anb`.
- Gate: `bash scripts/run_stdlib_gate.sh`. Tests: `cargo test -p anubis-compiler --lib phase5_`.

## NOW REAL (Phase-6 — package manager + proof-carrying dependencies, 2026-07-12)

- **`[dependencies]`** with registry SemVer / `path` / pinned-`git` / optional `registry =` URL — REAL
- **`Anubis.lock`** full **transitive** closure: version + content Merkle — REAL
- **Cycles / conflicts:** `ANUBIS_DEP_CYCLE`, `ANUBIS_DEP_VERSION_CONFLICT` — REAL
- **Content-addressed cache** + `ANUBIS_CACHE_HASH_MISMATCH` — REAL
- **Local + file:// + https registries** + `anubis package publish --key` — REAL
- **Proof composition:** signed `evidence/` + PCA + Ed25519 trust + **source binding** +
  **`summaries.json` re-derive** — REAL (`ANUBIS_DEP_PROOF_UNVERIFIED`, `ANUBIS_DEP_UNTRUSTED_SIGNER`)
- **Deps mount as modules** (direct + transitive); Phase 1–5 at consumer call sites — REAL
- **Evidence Merkle + dep_closure** (direct flag, transitive:true) — REAL
- Gate: `bash scripts/run_package_gate.sh`. Docs: `docs/language/PACKAGES.md`.
  Tests: `cargo test -p anubis-compiler --lib phase6_` (19+).

### Phase-6 residual boundaries (not gaps in the crown)

- Not crates.io/npm wire protocol — Anubis registry is local / `file://` / simple HTTP
  (`versions.txt` + tree or `.tar.gz`).
- Git requires `git` on PATH + pinned `rev`.
- Summaries are sealed and re-derived; call-site enforcement is live re-typecheck of mounted
  sources (Phase-3 taint interproc limits still apply).
- SemVer conflict is exact version/content clash on the same package name (not full PubGrub
  backtracking across a multi-version solution space).

## Explicitly PLANNED (not real yet — verified still-unshipped 2026-07-10)

- Array/list slicing **sugar** `xs[1..3]` (clean parse error today; use explicit list builtins)
- Async / await / tasks (language-level). **Governed I/O builtins** `read_file`/`write_file`/`open`/
  `send`/`connect`/`time`/`rand` are REAL executables (Phase-3 C3) via `std::fs`/`std::net`/
  `std::time`; they are still gated by mode + `uses(...)` in the checker
- Full IDE matrix beyond Phase-7 MVP (rename, completions, debugger) — **LSP diagnostics + contract hover are REAL** (`anubis lsp`, `editors/vscode-anubis`)
- Full-language self-host — **partially closed (2026-07-12).** The Anubis-SH self-host
  compiler now **compiles and runs the full executable language** (enums, `Name::Variant`,
  match, if-expressions, `for x in <collection>`, maps), verified byte-for-byte against the
  Rust host over the corpus (`bash scripts/run_selfhost_fulllang_gate.sh` →
  `SELFHOST_FULLLANG_GATE: PASS`). The stage0→1→2→3 bootstrap seal holds, now with a
  **same-toolchain binary fixpoint** (`bash scripts/run_selfhost_gate.sh`; see
  `docs/language/SELFHOST.md`). Still **not** claimed: (a) the compiler's *own source*
  rewritten to use full-language constructs and still fixpoint — `anubis_sh.anb` remains
  authored in the stable SH subset; (b) native lowering in Anubis (codegen emits an
  interpreter package, not host `lower_program_to_rust`); (c) cross-rustc-version binary
  identity; (d) Z3/taint engine in Anubis; (e) replacing the Rust host as the trusted
  default toolchain (a trusted seed is standard for bootstraps).
- Automatic remote exploit / ROP / C2 (out of scope by design — not a gap to “close” for A+)

## Current Gaps That Must Be Addressed in Fixtures / Tests (but are targeted for this slice)

- Missing standardized error codes on all type/parse failures (ANUBIS_*)
- Struct support (decl + literal + field) — **target: REAL by end of slice**
- Consistent column information in diagnostics
- `check --emit ast,hir,mir` (or equivalent via --evidence) producing stable JSON
- 25 fixture PASS/FAIL expectations + runner + repro
- CLI `run` story (documented shim or interpreter if native not emitted by default)

## What Must Never Be Claimed

- "General-purpose language complete" (Anubis targets a niche: a proof-carrying, evidence-native
  systems language — not a Python/Haskell/Swift replacement)
- Async or language-level networking (PLANNED). Multi-file modules and `import std.*` ARE real
  now (see NOW REAL) — do not claim them unsupported.
- Full public-registry ecosystem maturity / LSP / IDE tooling (local package + proof-carrying
  deps **are** real — Phase 6; see PACKAGES.md)

(Note: full enums — unit/tuple/struct variants — and built-in `Option`/`Result` + `?` **are** real now,
so they are no longer on this never-claim list; see the NOW REAL sections above.)

See CORE_FEATURES.md for the exact minimum that **is** supported and must be proven by the 25 fixtures + A15.
