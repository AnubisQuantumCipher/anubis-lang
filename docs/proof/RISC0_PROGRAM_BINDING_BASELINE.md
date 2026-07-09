# RISC0 Program Binding Baseline

**Branch:** `a-plus-maturity/20260705-1649`  
**Milestone commit:** `164488a` — *risc0: bind the proof to the actual Anubis program*  
**Recorded:** 2026-07-09

## Old gap (retired)

`prove --backend risc0` previously generated a **real** cryptographic receipt for a **hardcoded** guest:

```rust
// guest always x * 6; host always wrote 77
let x: u32 = env::read();
env::commit(&(x * 6));
```

The `.anb` file was effectively ignored for the proof body. That violated “code must prove itself.”

## Current state (input-free program binding = REAL)

`lower_program_to_guest` lowers the **actual Anubis program** into the RISC0 guest (`std` guest).  
`anb_main()` runs in the zkVM; the journal is `anb_main()`’s result as `u32`.  
risc0-build derives **ImageID from that guest ELF** → receipt is program-bound.

Verified cases (prior gate):

| Program | Journal (u32 LE) | Binding |
|---------|------------------|---------|
| `examples/proof_factorial.anb` | **120** = factorial(5) | guest contains `anb_factorial` / `anb_main` |
| `examples/proof_fib.anb` | **55** = fib(10) | distinct ImageID from factorial |

Gate: `bash scripts/run_proof_binding_gate.sh` → PASS when Metal/RISC0 reference is present.

## Limitation (this baseline)

Programs are **input-free**: constants are baked into source (`factorial(5)`).  
Host still has a residual `env_builder.write(&77u32)` for the **fallback** echo guest only; program-derived guests currently ignore executor env inputs.

**Next target:** parameterized proofs — `proof_input_u32("n")` + CLI `--input-json` so journal depends on supplied input while ImageID still binds to program.
