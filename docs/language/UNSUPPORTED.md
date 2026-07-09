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

## Explicitly PLANNED (not real yet)

- Full exhaustiveness errors in typecheck for `match` (runtime `_` / fail-soft still apply)
- Array/list slicing
- `Result<T,E>` / `Option` / error handling in the language surface (beyond assert/assume)
- Block comments `/* ... */`
- Full string type + operations beyond label literals (len, concat, etc. may be PARTIAL)
- u16 / u64 as first-class with full arithmetic in all paths (u32/u8 dominant)
- Module system with real name resolution, `use`, multiple files, stdlib imports
- Full `@safe fn foo() {}` decorator syntax with separate enforcement (blocks + Mode inference cover current needs)
- Generics / traits / impls
- Async / await / tasks / networking
- Package manager, crates, publishing
- LSP / IDE support
- Large standard library (beyond the 9 core builtins)

## Current Gaps That Must Be Addressed in Fixtures / Tests (but are targeted for this slice)

- Missing standardized error codes on all type/parse failures (ANUBIS_*)
- Struct support (decl + literal + field) — **target: REAL by end of slice**
- Consistent column information in diagnostics
- `check --emit ast,hir,mir` (or equivalent via --evidence) producing stable JSON
- 25 fixture PASS/FAIL expectations + runner + repro
- CLI `run` story (documented shim or interpreter if native not emitted by default)

## What Must Never Be Claimed

- "General-purpose language complete"
- Full enums, modules, Result, async, or stdlib
- Mature package / build / release story

See CORE_FEATURES.md for the exact minimum that **is** supported and must be proven by the 25 fixtures + A15.
