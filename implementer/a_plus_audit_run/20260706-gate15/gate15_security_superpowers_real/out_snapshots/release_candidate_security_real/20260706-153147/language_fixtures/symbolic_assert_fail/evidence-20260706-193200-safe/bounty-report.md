# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=1
- `symbolic`: PASS - constraints=2
- `solver`: FAIL - assert:(bvugt x (_ bv20 32))=FAIL
- `source_hash`: PASS - 01c7b90c4d2ee27ef57809ff0538aa35a93791ed2ec08fae28104c968a1a9d18
- `build_log_hash`: PASS - 84f3aad0f8162f9cbea4e740a69e9f569849da080bbcf0da124e1553bd0f8370
