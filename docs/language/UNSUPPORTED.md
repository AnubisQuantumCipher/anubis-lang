# Anubis Unsupported / PLANNED Surface (Gate 2/3 Truth)

**Rule:** Anything not implemented in this slice **must** be listed here and reflected in MATURITY_CLAIM_MATRIX.md + A_PLUS_ACCEPTANCE_CRITERIA.md. No over-claiming "general-purpose language complete".

## NOW REAL (previously planned — implemented 2026-07-09, see TURING_COMPLETENESS.md)

- `while` loops, `loop`, `break`, `continue` — REAL and executed by `anubis run`.
- Assignment / mutation (`x = expr;`) — REAL.
- General recursion and mutual recursion (real call stack) — REAL.
- Operators `/ % != && || !` and unary `-`/`!`, and `else if` chains — REAL.
- **The executable language is Turing-complete** (loops + mutation + recursion), with a
  runnable Turing-machine witness. Evidence: `bash scripts/run_turing_core_fixtures.sh`.

## Explicitly PLANNED (not real yet)

- Enums and tagged unions (`enum Color { Red, Blue(u32) }`)
- `for` loops (range / iterator form; use `while` today)
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
