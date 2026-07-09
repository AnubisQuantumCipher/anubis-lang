# Anubis Language Specification (Minimum Core for Gate 2/3 Slice)

**Scope:** First serious general-purpose language layer on top of the existing evidence-native compiler. Focus: make ordinary code feel like a real language while **never weakening** sealed Gates 4/5/7/8/10/11.

**Version for this slice:** v0.2-core (2026-07-06, a-plus-maturity/20260705-1649)

## Core Principles (non-negotiable)

- Safe-by-default: `@safe` / Safe mode rejects tainted flows without explicit declassify(policy, reason).
- Evidence everywhere: every check/build/prove produces or can produce tamper-verifiable bundles.
- Source of truth is the source + receipt/journal + bundle hashes. No hidden trust.
- Parser must not panic on bad input for supported fixtures; always emit usable diagnostics + spans.
- All special forms (symbolic, assume, assert, taint_source, declassify, sink) are first-class in AST/HIR/MIR and preserved in evidence.

## Minimum Supported Features (this slice)

1. Comments
   - `//` to end of line (real, stripped by lexer).

2. Function definitions
   - `fn main() { ... }`
   - Typed parameters: `fn add(x: u32, y: u32)`
   - Return types where declared in surface (partial inference supported).

3. Local bindings
   - `let x = expr;`
   - `let x: u32 = expr;`

4. Primitive types (real in this slice)
   - `bool`
   - `u8`
   - `u32`
   - `u16`, `u64`, `string`/`str`: PARTIAL (literals and limited use exist; full ops PLANNED — see UNSUPPORTED).

5. Expressions
   - Integer, boolean, string literals.
   - Variable references.
   - Arithmetic: `+ - * / %`
   - Comparisons: `== != < <= > >=`
   - Logical: `&& || !` (short-circuiting `&&`/`||`)
   - Unary: `-expr` (negation), `!expr` (logical not)
   - Bitwise: `&`
   - Parenthesized: `(expr)`

6. Control flow
   - `if cond { ... } else { ... }`, including `else if` chains — REAL.
   - `while cond { ... }`, `loop { ... }`, `break`, `continue` — REAL and executed by `anubis run`.
   - `for v in start..end { ... }` (range loop) — REAL and executed.
   - `return expr;` — REAL (functions return values; drives recursion).
   - Assignment `x = expr;` and indexed assignment `a[i] = expr;` — REAL.
   - Note: `if`/`while`/`for` header expressions do not parse a trailing `{` as a struct literal
     (Rust-style rule), so `while running {` and `for i in 0..n {` are unambiguous.
   - The language is Turing-complete at runtime: loops + mutation + recursion execute.
     See [TURING_COMPLETENESS.md](TURING_COMPLETENESS.md).

7. Structs (added in this slice)
   - `struct Point { x: u32, y: u32 }`
   - `let p = Point { x: 1, y: 2 };`
   - `p.x`

7b. Arrays / lists (REAL, executed by `anubis run`)
   - Literal: `let a = [1, 2, 3];`
   - Index read: `a[i]`; index write: `a[i] = v;`
   - Builtins: `len(a)` (list or string length), `push(a, v)` (grow a list)
   - Values are dynamically typed (Int / Bool / Str / List); lists may hold any mix and grow.
   - Enables real algorithms: sorting, dynamic programming, string scanning.

8. Enums / tagged unions: PLANNED (documented; not required for this slice's 25 fixtures).

9. Functions / calls
   - User-defined calls (arity + basic type checking).
   - Builtins (must be recognized and lower correctly):
     - `symbolic("name")`
     - `assume(e)`
     - `assert(e)`
     - `taint_source("label")`
     - `declassify(v, policy?, reason?)`
     - `sink(v)`

10. Attributes / effects (at minimum parse + preserve)
    - `@safe`, `@research`, `@proof`, `@audit`, `@effect(...)`
    - Enforcement may be partial; lowering to Mode + effects vec is real.

11. Modules / imports: minimal parse support or explicitly PLANNED (current: Import/Module items exist with recovery; full resolution PLANNED).

12. Error handling: Result-style minimal or PLANNED (none in surface language for this slice).

Anything not listed above or marked PARTIAL/PLANNED must be in UNSUPPORTED.md.

## Lowering & Evidence Contract

- Every `let` / call / control node produces HIR bindings + MIR blocks.
- Taint, declassify policy, symbolic constraints, solver obligations, and effects are recorded in TypedIR and emitted in evidence bundles.
- RISC0 path (`prove --backend risc0`) and Metal parity remain untouched in semantics.
- Parser diagnostics + type errors must be reproducible (modulo allowed nondet like timestamps) for ordinary fixtures.

## Verification

All 25 fixtures + runner + A15 must pass with the exact commands in the plan. Sealed gate regressions are forbidden.
