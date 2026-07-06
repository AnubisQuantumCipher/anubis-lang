# Anubis Bounty Evidence Report

- mode: fuzz
- lane: fuzz

## Checks

- `parse`: PASS - ok
- `typecheck`: PASS - mode=fuzz symbols=2 functions=1
- `taint`: PASS - labels=3 traces=1
- `symbolic`: PASS - constraints=1
- `solver`: PASS - solver:no-obligations=PASS
- `source_hash`: PASS - dbbd6fdbe2c8716380204bcdf8a7ae3468f6c87d4951b0ee0371d2ac9e884af0
- `build_log_hash`: PASS - 948394179540ca048055b35cfa5879356bfc51368e0b0a2ad2d05f1c420c7d83
