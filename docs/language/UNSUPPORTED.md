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
- Module system with real **multi-file** name resolution + stdlib imports (single-file `module {}`
  grouping works; the call namespace is flat; `import` parses but does not yet resolve across files)
- An Anubis-level standard library (`stdlib/` is empty; the ~150 builtins are baked into the Rust emitter)
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
- **Multi-file** modules, an Anubis-level stdlib, async, or language-level networking (all PLANNED above)
- Mature package / build / release story; LSP / IDE tooling

(Note: full enums — unit/tuple/struct variants — and built-in `Option`/`Result` + `?` **are** real now,
so they are no longer on this never-claim list; see the NOW REAL sections above.)

See CORE_FEATURES.md for the exact minimum that **is** supported and must be proven by the 25 fixtures + A15.
