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
- Arrays/lists: `[..]`, `len`, `push`, index R/W
- **Maps** `{ k: v }` + index get/set + for-in keys
- Structs: decl, literal, field access
- **Enums**: unit, tuple, **struct-like** variants
- **match** with bindings; **A+ exhaustiveness** (`ANUBIS_MATCH_NON_EXHAUSTIVE` without `_` or full arms)

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
- Full string API (beyond len/concat/index)
- Module/import name resolution across files
- Column-perfect diagnostics everywhere
- Return-type checking on all paths

## PLANNED (not claimed)
- Array/list slicing sugar
- `Option`/`Result` sugar beyond enums
- Async, networking language surface, large stdlib, LSP, packaging
- Automatic remote exploit chains / ROP (explicitly out of scope for PoC kit)

## Gates
| Gate | Command |
|------|---------|
| Turing core | `bash scripts/run_turing_core_fixtures.sh` |
| Enum/match | `bash scripts/run_enum_match_gate.sh` |
| For-in | `bash scripts/run_for_in_gate.sh` |
| Lang trio | `bash scripts/run_lang_trio_gate.sh` |
| PoC kit | `bash scripts/run_poc_kit_gate.sh` |
| Power | `bash scripts/run_power_gate.sh` |
