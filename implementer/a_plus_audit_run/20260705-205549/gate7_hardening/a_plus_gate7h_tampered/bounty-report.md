# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=1 functions=1
- `symbolic`: PASS - constraints=2
- `solver`: FAIL - assert:(bvugt x (_ bv20 32))=FAIL
- `source_hash`: PASS - 56d9f280aea9c0b5292686901fb8ab206dccf7a2c4eed41b999685045827b185
- `build_log_hash`: PASS - 5d4e69d9bdb2bfc7342ab2cc4c837f4ba0650f53eb1541d39cb03c4ddeb50abc
