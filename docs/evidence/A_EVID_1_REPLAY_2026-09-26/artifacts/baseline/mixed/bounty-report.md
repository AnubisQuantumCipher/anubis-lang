# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=1 mono=0
- `monomorphization`: PASS - static_specializations=0 (codegen remains AnubisValue-erased)
- `symbolic`: PASS - constraints=3
- `solver`: FAIL - assert:(= anb_x (_ bv1 64))=PASS,assert:(= anb_x (_ bv2 64))=FAIL
- `command_rejection`: FAIL - ANUBIS_ASSERTION_DISPROVED: 1 assertion(s) disproved by counterexample:
  assert:(= anb_x (_ bv2 64))
  counterexample:
    x = 0x0000000000000001  (1)
- `source_hash`: PASS - fa7f82cca826343b1ebad4145061e5e39de3194c47bdda14d663912f42593d3c
- `build_log_hash`: PASS - e32f0716bfbe92ec08c00795626699d806bc8c00d06eb109950d3922aa624225
