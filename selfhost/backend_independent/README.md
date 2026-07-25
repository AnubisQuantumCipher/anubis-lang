# Independent second architecture (author-diversity residual)

## What DDC already closes

`scripts/run_selfhost_ddc_gate.sh` proves **toolchain** diversity: rustc/LLVM vs
non-LLVM gcc/tcc on a C port of the interpreter + C parser.

## What remains (honest)

The C parser and C interpreter were **same-human** ports of the Rust reference.
DDC does **not** defend against a subversion present identically in both
hand-written sources (Wheeler TT vs author collusion).

## This directory

`token_scan.c` is a **third architecture**: a pure table-driven scanner for the
self-host token alphabet, written from the language surface (not a line-port of
`backend_c/anubis_sh_parse.c`). It is compiled with the same non-LLVM `$CC` as
the DDC cB lane and checked for token-stream agreement on a fixture corpus.

This advances **architecture** diversity. It does **not** claim a second human
author. TT-total still requires an independent stranger reimplementation.

## Gate

```bash
bash scripts/run_author_diversity_gate.sh
# AUTHOR_DIVERSITY_GATE: PASS  (architecture lane)
# residual: same-human authorship for full TT-total
```
