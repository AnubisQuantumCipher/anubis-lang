# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=1 mono=0
- `monomorphization`: PASS - static_specializations=0 (codegen remains AnubisValue-erased)
- `symbolic`: PASS - constraints=1
- `solver`: PASS - solver:no-obligations=PASS
- `source_hash`: PASS - 4d6ab161876804e1375310d76f5b9d399282716fe26c8eb95dce6e9913aa877b
- `build_log_hash`: PASS - d21e32c59525a827ce7f460ec222322e1c5c285a59f217bbf4d6d2495c2b0da6
