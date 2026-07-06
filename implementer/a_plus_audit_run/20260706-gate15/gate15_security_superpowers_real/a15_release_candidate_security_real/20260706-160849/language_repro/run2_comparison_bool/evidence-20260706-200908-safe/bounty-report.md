# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=1
- `symbolic`: PASS - constraints=3
- `solver`: PASS - assert:(bvugt x (_ bv0 32))=PASS,assert:(= x (_ bv5 32))=PASS
- `source_hash`: PASS - c3f240c1a79384391d36c8bdf1104e3833e09de260cfc47b4fb6d9461865a086
- `build_log_hash`: PASS - 73e2efb9b3f023ba93bbdad61b500cc866de533226c9b629ad922e91011efbca
