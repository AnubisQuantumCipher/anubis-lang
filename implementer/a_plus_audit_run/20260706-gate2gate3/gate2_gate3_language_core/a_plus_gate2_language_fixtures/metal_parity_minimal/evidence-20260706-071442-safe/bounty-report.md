# Anubis Bounty Evidence Report

- mode: safe
- lane: safe-check

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=safe symbols=2 functions=1
- `symbolic`: PASS - constraints=2
- `solver`: FAIL - assert:(= y (_ bv42 32))=FAIL
- `source_hash`: PASS - 000e356aecf0900290735166df52b875baaac43350d8f8d059287dda4fa4d4fa
- `build_log_hash`: PASS - afce094c679481efca4ff84845a80a68565be064e6e419acf4b48d3688a1317e
