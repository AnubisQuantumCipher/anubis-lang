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
  PLANNED note.) Still PLANNED: an Anubis-source stdlib reachable as `import std.*` (Phase 5).

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
  `taint_from_a_let_seed_is_conservatively_sticky_across_reassignment`,
  `interprocedural_return_taint_is_flagged_at_the_call_site`.

**Honest boundaries (deliberately deferred — this slice did NOT close them):**
- **Taint flow is reassignment-INSENSITIVE.** A binding's taint is fixed at its `let`/param seeding.
  Reassignment (`x = ...`) does not add or clear taint: a `let`-tainted var reassigned to a clean
  value stays conservatively tainted (fail-CLOSED, safe — clear it with `declassify(...)`), and a
  clean var reassigned to a tainted value is NOT re-tainted (a pre-existing fail-OPEN, unchanged by
  this slice). Making reassignment flow-sensitive needs proper control-flow-merge dataflow (branch
  snapshot/restore/join); three adversarial rounds confirmed a naive incremental version is unsound
  across `if`/`else`/loop bodies, so it is a separate future Phase-3 slice, not shipped half-working.
- **Interprocedural RETURN-taint is now modeled (Phase-3 slice 2).** A monotone fixpoint pre-pass
  (`compute_tainting_fns`, run before per-function analysis) marks each function whose return value
  carries INTERNAL taint — a `taint_source()`/`tainted<T>` local returned directly (through let-chains
  and casts), or a return of another marked function. `expr_taint_source`'s `Call` arm consults it, so
  `sink(get_secret())` is now flagged even with no tainted argument. The return-taint walk is
  scope-aware (respects lexical block shadowing) and declassify-aware (a function that declassifies
  before returning is clean). Also this slice: taint now propagates through an `as` cast (a new
  `Expr::Cast` arm — `sink(s as u64)` no longer launders). Still NOT modeled interprocedurally:
  **argument→return pass-through summaries** (calling `fn wrap(x){return x;}` with a tainted arg IS
  caught by the existing per-argument check, but `wrap` is not summarized as "returns taint iff arg N
  is tainted"), **parameter→sink summaries** (a callee that sinks its argument internally), and
  **higher-order / indirect calls** (`let f = get_secret; sink(f())` — the summary keys on the callee
  NAME; a function-valued variable is not resolved, same boundary as method calls via `CallExpr`).
- **Block-scoped shadowing is respected by the return-taint summary but NOT yet by the intra-procedural
  sink check.** The interprocedural walk snapshots/restores scope around blocks, so
  `fn f(c){ let x=5; if c { let x=taint(); } return x; }` is correctly clean. The *inline* equivalent
  (`let x=5; if c { let x=taint(); } sink(x);`) is still a pre-existing FALSE POSITIVE in
  `analyze_stmts`, which keys taint on a flat per-name scope without block push/pop. Fixing
  `analyze_stmts`' block scoping is a separate slice; it is a fail-CLOSED over-rejection (safe
  direction), not a leak.
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

## Explicitly PLANNED (not real yet — verified still-unshipped 2026-07-10)

- Array/list slicing **sugar** `xs[1..3]` (clean parse error today; use explicit list builtins)
- An Anubis-level standard library (`stdlib/` is empty; the ~150 builtins are baked into the Rust
  emitter). Multi-file `import` now resolves (see NOW REAL below) but there is no `import std.*`
  prelude yet — that is Phase 5.
- Async / await / tasks / language-level networking
- Package manager, crates, publishing
- LSP / IDE support
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
- An Anubis-level stdlib (`import std.*`), async, or language-level networking (all PLANNED above).
  Multi-file modules ARE real now (see NOW REAL) — do not claim them unsupported.
- Mature package / build / release story; LSP / IDE tooling

(Note: full enums — unit/tuple/struct variants — and built-in `Option`/`Result` + `?` **are** real now,
so they are no longer on this never-claim list; see the NOW REAL sections above.)

See CORE_FEATURES.md for the exact minimum that **is** supported and must be proven by the 25 fixtures + A15.
