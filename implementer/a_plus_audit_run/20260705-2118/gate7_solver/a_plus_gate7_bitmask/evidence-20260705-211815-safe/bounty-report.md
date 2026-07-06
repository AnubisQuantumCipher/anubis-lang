# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=2 functions=1
- `symbolic`: PASS - constraints=2
- `solver`: FAIL - assert:(= masked (_ bv44 32))=FAIL
- `source_hash`: PASS - 177c2202d0a6ea6fd7042b0df552b7e2cc2ac72aebae3fa94e52856f65ac8c98
- `build_log_hash`: PASS - ce32e2ed8ba6b5157c7146242f9fca9cc579fd2ec8033d1684adb4bb82e2088a
