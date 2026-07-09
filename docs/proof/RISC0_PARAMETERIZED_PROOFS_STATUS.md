# RISC0 Parameterized Proofs — Status

**Status: REAL (verified 2026-07-09)**  
Prime law: no claim without evidence.

## What works

```bash
anubis prove examples/proof/proof_factorial_input.anb \
  --backend risc0 --lane cpu \
  --metal-reference /Users/sicarii/Desktop/metal-hybrid-prover \
  --input-json '{"n":5}' \
  --evidence --out out/proof_factorial_5
```

| Input | Journal | verify | ImageID vs n=6 |
|-------|---------|--------|----------------|
| `{"n":5}` | **120** | passed | **same** as n=6 |
| `{"n":6}` | **720** | passed | — |

- Guest is program-derived (`anubis_load_proof_inputs`, `anubis_proof_input_u32_val`, `anb_factorial`) — not `x*6`.
- `parameterized: true`, `input_sha256` differs across inputs.
- CLI: `--input-json` and `--input-file` (exclusive).

## ABI

See `RISC0_PARAMETERIZED_INPUT_ABI.md`. Builtin: `proof_input_u32("key")`, `proof_input_bool("key")`.

## Gate

```bash
bash scripts/run_parameterized_proof_gate.sh --out out/parameterized_proof
```

## Honesty boundary

- Journal is `u32` LE commit of `main()` return (v1).
- Nested JSON / strings as proof inputs: unsupported (fail closed).
- Private/redacted inputs: schema field present; redaction path not yet used.
