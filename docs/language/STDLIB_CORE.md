# Minimal Stdlib Core (Gate 2/3)

Classifications for this slice only.

- print : PARTIAL (supported by `anubis run` safe subset; not a broad stdlib yet)
- sink : REAL (recognized in taint/safe enforcement)
- taint_source(label) : REAL
- declassify(v [, policy, reason]) : REAL (policy+reason required in safe)
- symbolic(name) : REAL
- assume(e) : REAL
- assert(e) : REAL
- len(x) : REAL in `anubis run` (list/string length)
- hash_sha256 : PLANNED

### PoC kit (requires `anubis run --allow-research`)

- p8 / p16 / p32 / p64 : REAL (little-endian pack → byte list)
- cyclic(n) : REAL
- flat(x) : REAL
- target_run(path, payload) : REAL (local process only; network forbidden)
- list + list : REAL (payload concatenation)

See `docs/language/POC_KIT.md` and `bash scripts/run_poc_kit_gate.sh`.

All analysis builtins lower to special Expr forms and are captured in evidence.
