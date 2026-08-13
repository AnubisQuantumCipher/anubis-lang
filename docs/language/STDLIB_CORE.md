# Stdlib Core — analysis, proof & PoC builtins

This document covers only the **analysis / proof / PoC** builtin subset (taint, symbolic,
declassify, proof-surface, PoC-kit).

**Complete name count is 213** (union of five sets in `compiler/src/backends/run.rs`). The full
table — including crypto, capability, and previously undocumented callables — is
[`BUILTINS.md`](BUILTINS.md). Do not use the older "~150" (README) or "~116 general-purpose"
figures; they understated the surface a stranger cannot invent by guesswork.

The general-purpose core is still narrated in `LANGUAGE.md` ("Standard library"). An Anubis-source
standard library ALSO exists over these primitives: **13** content-locked modules under
`compiler/stdlib/std/` (`math`, `collections`, `iter`, `result`, `option`, `io`, `str`, `crypto`,
`testing`, `pwn`, **`time`**, **`net`**, **`rand`**), imported via `import std.<module>` and
exercised by `scripts/run_stdlib_gate.sh`.

**Runtime fail-closed (stdlib edge cases):** the embedded runtime fails closed with explicit
`ANUBIS_*` panics on empty-collection/domain/type misuse. Gate:
`bash scripts/run_stdlib_failclosed_gate.sh` → **104/104 PASS** (2026-07-27). Do not document silent
`0` returns for `first`/`pop`/`min`/`find`/wrong-type `map`/etc. — that was the pre-patch defect class.

- print / println / eprint / eprintln : REAL (general-purpose I/O in `anubis run`)
- sink : REAL (recognized in taint/safe enforcement)
- taint_source(label) : REAL
- declassify(v [, policy, reason]) : REAL (policy+reason required in safe)
- symbolic(name) : REAL
- assume(e) : REAL
- assert(e) : REAL
- len(x) : REAL in `anubis run` (list/string length)
- hash_sha256 : REAL (alias of `sha256` / `sha256_hex` → `anubis_sha256`; NIST vector unit in run.rs)

### Proof surface (prove --backend risc0; also native run stubs)

- proof_input_u32 / proof_input_bool : REAL (guest env::read map; run via `ANUBIS_PROOF_INPUTS=k=v`)
- proof_commit_u32("name", v) : REAL (named public journal field)
- proof_commit_bool("name", v) : REAL (commits 0/1)
- proof_assert(cond) : REAL (false → panic / no receipt)
- journal_fields host decode : REAL (`journal_decoded.json` + metadata schema 1.4)

### PoC kit (requires `anubis run --allow-research`)

- p8 / p16 / p32 / p64 : REAL (little-endian pack → byte list)
- cyclic(n) : REAL
- flat(x) : REAL
- target_run(path, payload) : REAL (local process only; network forbidden)
- list + list : REAL (payload concatenation)

See `docs/language/POC_KIT.md` and `bash scripts/run_poc_kit_gate.sh`.

All analysis builtins lower to special Expr forms and are captured in evidence.
