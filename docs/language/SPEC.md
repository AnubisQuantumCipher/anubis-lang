# Anubis Language Specification

**Authoritative narrative reference:** [`../../LANGUAGE.md`](../../LANGUAGE.md).  
**Honest non-claims:** [`UNSUPPORTED.md`](UNSUPPORTED.md).  
**Adoption path:** [`TUTORIAL.md`](TUTORIAL.md).

**Version:** v0.3+ (Phases 1–7; 2026-07). This file remains a normative sketch for modes, evidence, contracts, and packages. Where it conflicts with `LANGUAGE.md` + live gates, **the code and gates win**.

**Historical note:** An earlier “v0.2-core Gate 2/3” draft lived here; it under-described enums, modules, stdlib, packages, and DX. Prefer `LANGUAGE.md` for full syntax.

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

8. Enums / tagged unions: REAL (unit, tuple, struct-like variants + match; exhaustiveness A+).
   Maps `{k:v}`, if-expressions with required `else`: REAL.

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

11. Modules / imports: multi-file resolve + package dep mounts — REAL (Phase 5/6). See `PACKAGES.md`.

12. Error handling: Result-style surface — PARTIAL / see UNSUPPORTED for gaps.

Anything not listed above or marked PARTIAL/PLANNED must be in UNSUPPORTED.md.

## Phase 7 — Developer experience (normative)

These surfaces are part of the language product, not optional demos.

1. **Comments** — `//` and nested `/* */` are real lexer tokens (`Token::LineComment` /
   `BlockComment`). Parse path strips them; `associate_docs` maps leading contiguous
   comments onto item start offsets for `anubis doc`.

2. **`anubis doc`** — Renders public (and optionally private) functions with Signature,
   **Contracts** (from AST `requires`/`ensures`), and Effects (`uses(...)`). Formats:
   Markdown (default), JSON. Fail-closed on typecheck errors of the entry graph.

3. **`anubis repl`** — Every entry is typechecked and obligation-checked before eval.
   Default evaluator: AST interpreter. `--exact` uses the same native lowering as
   `anubis run`. `--eval` is non-interactive; type failures exit non-zero.

4. **`anubis lsp`** — Stdio Language Server. Diagnostics from parse + typecheck +
   `SymbolicEngine::check_obligations`. Hover shows signature + Contracts. Completions /
   rename are out of MVP (UNSUPPORTED).

5. **Editors** — `editors/tree-sitter-anubis` (highlight grammar; not parser of record);
   `editors/vscode-anubis` (TextMate + language client launching `anubis lsp`).

6. **Tutorial** — `docs/language/TUTORIAL.md` is the prose adoption path; fixtures under
   `tests/fixtures/dx/` are gate-backed.

Gate: `bash scripts/run_dx_gate.sh` must print `DX_GATE: PASS`.

## Integer semantics (normative)

Every integer value is an `i64` at runtime. Annotations differ only in the *boundary guarantee* the
runtime enforces and the checker may therefore soundly assume:

- **Signed / default** (`int`, `i8`, `i16`, `i32`, `i64`): unbounded `i64`. No boundary coercion. The
  checker models the value as a 64-bit two's-complement bit-vector with signed comparisons — it may be
  negative, so a contract needing non-negativity must state `requires(x >= 0)`.
- **Unsigned fixed-width** (`u8`, `u16`, `u32`): a **parameter** is masked to `[0, 2^w)` at the call
  boundary (`v & (2^w − 1)` — e.g. `−1` becomes `2^32 − 1`, an oversized value its value mod `2^w`).
  Because the runtime *enforces* the range, the checker soundly assumes `0 ≤ x < 2^w` for the param —
  so `ensures(result >= 0)` and similar hold without a hand-written `requires(x >= 0)`. A caller passing
  an out-of-range argument has it masked identically when the callee's `requires`/`ensures` is composed
  at the call site, so composition matches the runtime.
- **`u64`**: unbounded `i64` (its `[0, 2^64)` range does not fit the non-negative `i64` range, so it is
  not boundary-coerced); it keeps the signed-model tax.

**Arithmetic wraps at `i64` (`2^64`), not at the annotated width.** A `u32` *return* or *local* is NOT
re-masked — only parameters are coerced (that is where the non-negativity tax lives, and `u32` is the
canonical integer spelling, so masking returns would silently change programs that return a
negative/overflowing value). Consequently `fn f(x: u32) -> u32 { return x + x }` returns `2·x` (which may
exceed `2^32`), not `(2·x) mod 2^32`. A function that needs a width-bounded result must state it
(`ensures(result < 4294967296)`) or route the value back through a `u32` parameter, which re-masks it.

Overflow policy: **wrap** (no checked-reject). The solver models `+ − * << >>` and `/ %` (guarded
non-zero divisor) exactly as `i64::wrapping_*`, so a `check`-proved arithmetic contract holds at runtime.
A contract that can wrap at `i64::MAX` (e.g. an unbounded `int` param's `x + 1 > x`) is correctly
*not* provable; the same contract on a `u32` param *is* provable because the mask keeps `x < 2^32`, so
`x + 1 < 2^33 < 2^63` cannot wrap.

## Contract checking: paths and refusals (normative)

`check` checks every `requires` of a called function, every `assert`, and every wrap-safety obligation
under the facts that hold **on the path that reaches it**. It has three outcomes: proved, disproved
(with a counterexample), or refused as **UNDECIDED**. Unknown is never treated as "the precondition
holds".

- **Guards and early exits.** In the then-branch, `if c` contributes `c`; in the else-branch it
  contributes `not c`. `while c` contributes `c` in its body. For `for v in a..b`, `a <= v < b` holds in
  the body only when `a` and `b` are modelable and neither is written in the body. A path ends at
  `return`, `break`, `continue`, `?`, or a call of the builtin `panic(x)` / `exit(x)` with exactly one
  argument, provided no user function or local of that name exists. Code after such an exit is
  unreachable, so `if bad { return; }` relieves every later obligation.
- **User functions are not assumed to diverge.** A call to a user function never ends the caller's
  path, even when the function always panics (a precision gap, P-DIVERGE-1). A user function named
  `panic` shadows the builtin and returns normally (matrix `pp_user_defined_panic_not_diverging`).
  `exit()` with no argument is a no-op at runtime and does not end the path
  (`pp_exit_without_arg_not_diverging`). A user function that *may* exit
  (it reaches `panic`/`exit`/`?`, directly or transitively) makes an enclosing range loop's variable
  unconstrained after the loop.
- **Checked narrowing** is the supported idiom for bounding a value the checker cannot follow:
  `if v < 0 || v > 100 { panic("out of range"); }` (or `return`/`break`). After the guard,
  `0 <= v <= 100` is a fact.
- **Joins.** After `if`/`match`, the state is the disjunction of the end facts of the branches that
  reach the join. An integer `let` bound to an `if`/`match` value takes the disjunction of its arm
  values.
- **Loops and scopes over-approximate; they never drop.** An integer written inside a loop keeps its
  name, but its facts are lost (it is havoced), so a later obligation over it cannot be proved from
  stale facts. When an obligation fails *only* because of a havoced value, or because of a parameter
  modeled only for its calls, `check` refuses it as UNDECIDED (evidence proof status
  `undecided_overapproximated`, no counterexample). It is not reported as disproved, because the
  counterexample may be unreachable. Loop-invariant step obligations are exempt: their counterexample
  refutes inductiveness itself.
- **Unencodable preconditions are refused.** If a called function's `requires` cannot be encoded at a
  call site (an argument or clause is not modelable, e.g. an untyped parameter), the obligation is
  `requires-unresolved@…` and `check` fails as UNDECIDED. It is never dropped.
- **Call-site folding.** A read of a literal struct field is folded to its value. A call to an
  *expression function* is unfolded at the call site. An expression function is a free function
  whose whole body is one `return <expr>` or one tail expression. It is unfolded only when all of
  these hold:
  - it is non-generic and not recursive, and unfolding nests at most 4 deep;
  - its parameters are untyped, `int`, `i64`, `string`, or `bool`;
  - its return type is absent, `int`, `i64`, `string`, or `bool` (so parameter passing and
    return are the identity at runtime);
  - its body reads only its parameters (no free variables);
  - its body never calls a parameter by name, and contains no lambda and no call of a computed
    callee.

  The result of any other call is opaque (unconstrained).
- **Wrap safety is per statement.** Each arithmetic operation's no-wrap check uses the facts that hold
  immediately before its statement. Facts established later (a later assignment, a loop's
  post-state, an in-body invariant) never justify it.

History: before 2026-09-23 (`d90082d0`):
- direct-call preconditions over unmodeled parameters were silently not checked (M-DIRECT-REQ);
- facts from branches and loops could leak past them;
- wrap checks used the end-of-body state.

Programs that relied on those gaps are now UNDECIDED or DISPROVED; the six shipped examples that did
were repaired with checked narrowing.

## Implementation limits (normative)

Limits are part of the language surface: exceeding one is a diagnostic, never a crash, a hang, or a
silent pass. A limit may be raised in a later release; lowering one is a compatibility change.

| Limit | Value | On exceeding it |
|---|---|---|
| Syntactic nesting depth (expressions, statements, patterns, string interpolation, counted together) | 256 | parse error `program is nested too deeply (more than 256 levels)`; `check` exits 1 with `verdict: "fail"` |

History: before 2026-09-23 there was no nesting bound. Nesting between 257 and a few thousand
levels was accepted; deeper input exhausted the stack and aborted the compiler (`SIGABRT`). The bound
is a deliberate compatibility change for programs nested more than 256 deep, recorded as matrix case
`limit_nesting_1000`.

Known lowering limit (not yet a diagnostic at `check`): an array literal nested more than about 128
levels fails native lowering (`ANUBIS_UNSUPPORTED_NATIVE_LOWERING`) because the generated Rust exceeds
rustc's default macro recursion limit. Tracked as N-LOWER-1.

## Lowering & Evidence Contract

- Every `let` / call / control node produces HIR bindings + MIR blocks.
- Taint, declassify policy, symbolic constraints, solver obligations, and effects are recorded in TypedIR and emitted in evidence bundles.
- RISC0 path (`prove --backend risc0`) and Metal parity remain untouched in semantics.
- Parser diagnostics + type errors must be reproducible (modulo allowed nondet like timestamps) for ordinary fixtures.

## Verification

Sealed gate regressions are forbidden. Primary DX seal: `bash scripts/run_dx_gate.sh`.
Phase 5/6 package and crypto gates remain independent regression surfaces.
