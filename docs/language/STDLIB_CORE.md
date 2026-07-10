# Stdlib Core — analysis, proof & PoC builtins

This document covers only the **analysis / proof / PoC** builtin subset (taint, symbolic,
declassify, proof-surface, PoC-kit). The **general-purpose** builtin surface is much larger —
~116 builtins (conversions, ~24 math, ~19 string, ~30 list, map, and higher-order functions) all
REAL in `anubis run` — and is documented authoritatively in `LANGUAGE.md` ("Standard library"),
which matches the codegen in `backends/run.rs` (`emit_builtin_call`) 1:1. There is no separate
`stdlib/` source tree yet (Phase 5 of the roadmap adds an Anubis-source stdlib over these
primitives).

- print / println / eprint / eprintln : REAL (general-purpose I/O in `anubis run`)
- sink : REAL (recognized in taint/safe enforcement)
- taint_source(label) : REAL
- declassify(v [, policy, reason]) : REAL (policy+reason required in safe)
- symbolic(name) : REAL
- assume(e) : REAL
- assert(e) : REAL
- len(x) : REAL in `anubis run` (list/string length)
- hash_sha256 : PLANNED

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
