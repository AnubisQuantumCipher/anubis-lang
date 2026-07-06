# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=2 functions=1
- `symbolic`: PASS - constraints=3
- `solver`: FAIL - assert:(= y (_ bv0 32))=FAIL
- `source_hash`: PASS - a5d1b23fa8d532dbcb5dbecbbf88474b597bb20620e6088277459c1433199f18
- `build_log_hash`: PASS - 766700ed1cbc095e0c3e9a056a16a5cacb947cf05e70b4e72c41fd42966a31f2
