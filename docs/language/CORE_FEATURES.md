# Anubis Core Language Features

**Truth date:** 2026-07-09 · branch `a-plus-maturity/*`  
This document lists what is **REAL** (implemented + tested), **PARTIAL**, or **PLANNED**.

## REAL — executable by `anubis run` / check / prove

### Core program structure
- Line comments `//`
- `fn` definitions with typed params (`u32`, `bool`, …)
- `let x = e;`, `let x: T = e;`
- Attributes: `@safe` / `@research` / `@poc` / `@fuzz` / `@proof` / `@audit` / `@effect` (parse + mode inference; research requires `authorization=`)

### Types & checking (A+)
- Primitives: `bool`, `u8`, `u16`, `u32`, `u64` (numeric widths interoperate)
- String literals + concat
- **Call-site type checks** against parameter annotations (`ANUBIS_TYPE_MISMATCH`, `ANUBIS_ARITY_MISMATCH`)
- **Let/assign** annotation vs inferred init type
- `tainted<T>` is a *qualifier* (compatible with `T` for annotation matching; taint flow still separate)
- Integer literals: decimal, `_` separators, **`0x` / `0b` / `0o` radix**

### Expressions & control
- Arithmetic `+ - * / %`, comparisons, logical `&& || !`, unary `-` / `!`, bitwise `& | ^ << >> ~`
- `if` / `else if` / `else` statements
- **if-expressions** `let x = if c { a } else { b }` (`else` required)
- `while`, `loop`, `break`, `continue`, `for i in a..b`, `for x in collection`
- Assignment + indexed assignment; nested places (`a.b[i]`)
- Recursion / mutual recursion (real call stack)

### Data
- Arrays/lists: `[..]`, `len`, `push`, index R/W (**fail-closed** OOB — `xs[i]`/`m[k]` trap; use `get`/`has_key`)
- **Maps** `{ k: v }` + index get/set + for-in keys
- Structs: decl, literal, field access
- **Enums**: unit, tuple, **struct-like** variants; construction **validated** (`ANUBIS_UNKNOWN_ENUM`/`ANUBIS_UNKNOWN_VARIANT`)
- **match** with bindings; **A+ exhaustiveness** (`ANUBIS_MATCH_NON_EXHAUSTIVE` without `_` or full arms)

### Abstraction, generics, error handling
- **Closures** `|x| expr` / `|x, y| { … }` — first-class, capturing; direct-call arity checked
- **Traits + `impl`** — default methods, overrides, inherent-beats-trait resolution
- **Generics** — parse and erase (`Box<T>`, `trait A<T>`); type params are dynamically checked at runtime
- **`Option` / `Result` + `?`** — built-in `Some`/`None`/`Ok`/`Err`; `?` short-circuits on `None`/`Err`
- **Block comments** `/* … */` (nesting-aware), in addition to line comments `//`
- **`input()` / `read_line()`** — read a line from stdin (forwarded to the run binary)
- **~150 builtins** across string/list/map/math/functional/io — see `STDLIB_CORE.md`

### Turing completeness
- REAL — see `TURING_COMPLETENESS.md` + `scripts/run_turing_core_fixtures.sh`

### Analysis / safe surface
- `symbolic` / `assume` / `assert` / `taint_source` / `declassify` / `sink`
- Raw pointer reject in safe; effect forbid for shell/network in safe

### Proof surface (`anubis prove --backend risc0`)
- `proof_input_u32` / `proof_input_bool`, `proof_commit_u32` / `proof_commit_bool`, `proof_assert`
- Named journal fields + multi-field commit

### PoC kit (`anubis run --allow-research`)
- Packing `p8`/`p16`/`p32`/`p64`, `cyclic`, `flat`, list concat
- **`target_run` → TargetRun** named fields (`crashed`, `signal`, `exit_code`, `payload_len`, `timed_out`) + index compat
- Process mutation fuzz; network targets fail-closed
- See `POC_KIT.md`

## PARTIAL
- Column-perfect diagnostics everywhere (line:col + caret exist; not every path is column-perfect)
- **Return-type checking** — a *literal* return of an unambiguously wrong type is now rejected
  (`fn f() -> u32 { "s" }` → `ANUBIS_RETURN_TYPE_MISMATCH`). Dynamic returns (variables, calls,
  if/match) are still unchecked at full flow depth — residual of the structured-type arc
- **Monomorphized code clones** — checker inventory + `anb_*__mono__*` specialized clones for
  literal-pinned generic calls; values still `AnubisValue` (unboxed monomorphs residual)

## REAL (modules / stdlib / packages — do not claim “planned”)
- Multi-file `import a.b;` resolve + `import std.*` content-locked stdlib (13 modules)
- Packages / lock / trust spine — `docs/language/PACKAGES.md`
- Network / time builtins + `std.net` / `std.time` wrappers (effects: `net.send`, `time.now`)

## PLANNED (not claimed)
- Array/list slicing **sugar** `xs[1..3]` (use explicit list builtins today)
- Unboxed monomorphized native codegen (inventory already real)
- Async language surface, full LSP product surface
- Automatic remote exploit chains / ROP (explicitly out of scope for PoC kit)

Authoritative completeness map: `docs/language/LANGUAGE_COMPLETENESS.md`.

## Gates
| Gate | Command |
|------|---------|
| Turing core | `bash scripts/run_turing_core_fixtures.sh` |
| Enum/match | `bash scripts/run_enum_match_gate.sh` |
| For-in | `bash scripts/run_for_in_gate.sh` |
| Lang trio | `bash scripts/run_lang_trio_gate.sh` |
| PoC kit | `bash scripts/run_poc_kit_gate.sh` |
| Power | `bash scripts/run_power_gate.sh` |
