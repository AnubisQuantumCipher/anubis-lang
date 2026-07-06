# Anubis Core Language Features (Gate 2/3 Minimum)

This document lists what is REAL (implemented + tested by fixtures/runner), PARTIAL, or PLANNED for the current slice.

## REAL (exercised by 25 fixtures + unit tests + sealed regressions)

- Line comments `//`
- `fn` definitions with typed params (u32 etc.)
- `let x = e;`, `let x: u32 = e;`
- Primitives: bool, u8, u32 (literals + ops)
- Expressions: literals (int/string/bool), vars, + - *, comparisons, &, calls, parens
- Control: if/else, return (basic)
- Structs: decl, literal, field access (added/required in slice)
- Calls: user fns + builtins symbolic/assume/assert/taint_source/declassify/sink
- Special lowering: taint tracking, declassify policy+reason enforcement, symbolic constraints, solver obligations, assert/assume
- Attributes: parse + preserve @safe/@research/@proof/@audit/@effect (via Mode + effects)
- Evidence: AST/HIR/MIR JSON emission, bundle + verify, RISC0 receipts, Metal parity
- Diagnostics: source spans, file/line/column (improved), human messages + new ANUBIS_* codes

## PARTIAL (exists in some form, needs strengthening for slice)

- u16 / u64 / string primitives (some tests use u8/string labels; full width + string ops limited)
- Module/import (parsed with recovery; no real name resolution or stdlib import)
- Column-accurate diagnostics (byte spans dominant; improve to line+col)
- Return type checking + missing return errors

## PLANNED (explicitly not required / out of scope for this slice)

- Enums / tagged unions (document in UNSUPPORTED)
- while / for loops (if not trivial to add; provide working if/else coverage)
- Result / error handling types in language surface
- Block comments `/* */`
- Large stdlib (only the 9 minimal builtins listed in plan)
- Full attribute decorator syntax with enforcement (parse/preserve is enough)
- LSP, packaging, async, networking, public release

All PLANNED items must appear in UNSUPPORTED.md and the claim matrix as NO / PARTIAL.
