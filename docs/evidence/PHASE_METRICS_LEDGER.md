# Anubis phase-metrics ledger

Append-only observations. New measurements are emitted by
`bash scripts/phase_metrics.sh --append-ledger`. Each block is bound to the tree, commit, branch,
and dirty count printed inside it.

## Phase 0 start — timestamp not captured

Command: `bash scripts/phase_metrics.sh` · exit: `0`

This block was captured before the ledger option existed and inserted verbatim from the retained
phase-start output. Its printed tree, commit, branch, and dirty count are the provenance.

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 9c4e38304c053e6271886cb73fa67fe297bd73c3
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 143 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
source-walker pair similarity                   69%   0% (one implementation)
  ^ lines in the pair                          1247   ~half
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-07-30T05:13:28Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 9c4e38304c053e6271886cb73fa67fe297bd73c3
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 147 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
source-walker pair similarity                   69%   0% (one implementation)
  ^ lines in the pair                          1247   ~half
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-07-30T05:32:00Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 9c4e38304c053e6271886cb73fa67fe297bd73c3
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 151 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                           602   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  2498   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-07-30T05:40:21Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 9c4e38304c053e6271886cb73fa67fe297bd73c3
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 151 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                           602   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  2498   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-07-30T07:46:13Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 0e910c9bb2e83438696eaaf0f49d0e3c5e658960
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 156 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                           602   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  2498   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-07-30T08:40:05Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 0e910c9bb2e83438696eaaf0f49d0e3c5e658960
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 154 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                           602   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  2498   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-07-30T13:27:54Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 0e910c9bb2e83438696eaaf0f49d0e3c5e658960
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 163 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                           602   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  2498   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-07-30T16:14:41Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /Users/sicarii/anubis-lang
commit    : 0e910c9bb2e83438696eaaf0f49d0e3c5e658960
branch    : a-plus-maturity/safe-mode-trust-spine-20260725
dirty     : 173 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28801   strictly decreasing (Phase 2+)
  pair: source walkers                         1247   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                           602   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  2498   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1247   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   19   -
_ => in label-lane walkers                       12   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=0   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   5   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-08-13T15:58:03Z

Command: `bash scripts/phase_metrics.sh` · exit: `0`

```text
═══ PHASE METRICS ═══
tree      : /private/tmp/anubis-p2-completion
commit    : 9da568fdbb17a06a5f5ba78d17a65dbf7adb203a
branch    : docs/phase-2-completion-2026-08-13
dirty     : 1 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28826   strictly decreasing (Phase 2+)
  pair: source walkers                         1267   lines across both siblings
  pair: pattern seeders                          81   lines across both siblings
  pair: return summaries                        568   lines across both siblings
  pair: block walkers                            38   lines across both siblings
duplicated lane pairs                             4   0
  ^ lines in duplicated pairs                  1954   decreasing
source-walker pair similarity                   69%   diagnostic; pair count decides
  ^ lines in the source pair                   1267   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   16   -
_ => in label-lane walkers                        4   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=1   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   4   non-increasing, → 1
general ExprStmt arm: walk_block_taint           NO   yes
general ExprStmt arm: walk_block_secret          NO   yes

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-08-13T22:50:00Z

Command: `bash scripts/phase_metrics.sh` · exit: `0` · tree: `/tmp/anubis-main-1499` · commit: `1499f607a107a49e340c9c7e97e200cb9b83b32e` · dirty: `0`

```text
═══ PHASE METRICS ═══
tree      : /tmp/anubis-main-1499
commit    : 1499f607a107a49e340c9c7e97e200cb9b83b32e
branch    : HEAD
dirty     : 0 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28882   strictly decreasing (Phase 2+)
  pair: source walkers                         1187   lines across both siblings
  pair: pattern seeders                     removed   shared implementation expected
  pair: return summaries                        579   lines across both siblings
  pair: block walkers                     delegated   thin adapter over walk_block_labels
duplicated lane pairs                             2   0
  ^ lines in duplicated pairs                  1766   decreasing
source-walker pair similarity                   67%   diagnostic; pair count decides
  ^ lines in the source pair                   1187   decreasing
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   16   -
_ => in label-lane walkers                        0   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=1   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   4   non-increasing, → 1
general ExprStmt arm: walk_block_taint          yes   via walk_block_labels
general ExprStmt arm: walk_block_secret         yes   via walk_block_labels

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-08-13T23:30:00Z

Command: `bash scripts/phase_metrics.sh` · exit: `0` · tree: `/tmp/anubis-p2-returns` · commit: `3e6c0720ef3c22ac8fbab826f23c4b81b5b90fbc` · dirty: `0` · not origin/main

```text
═══ PHASE METRICS ═══
tree      : /tmp/anubis-p2-returns
commit    : 3e6c0720ef3c22ac8fbab826f23c4b81b5b90fbc
branch    : phase2/slice-source-walkers
dirty     : 0 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28339   strictly decreasing (Phase 2+)
  pair: source walkers                      removed   shared implementation expected
  pair: pattern seeders                     removed   shared implementation expected
  pair: return summaries                    removed   shared implementation expected
  pair: block walkers                     delegated   thin adapter over walk_block_labels
duplicated lane pairs                             0   0
  ^ lines in duplicated pairs                     0   decreasing
source-walker pair similarity               removed   shared implementation expected
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   16   -
_ => in label-lane walkers                        0   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=1   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   4   non-increasing, → 1
general ExprStmt arm: walk_block_taint          yes   via walk_block_labels
general ExprStmt arm: walk_block_secret         yes   via walk_block_labels

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```

## 2026-08-14T01:29:00Z

Command: `bash scripts/phase_metrics.sh` · exit: `0` · tree: `/tmp/anubis-main-1499` (detached `origin/main`) · commit: `6dcfa35363d84cdeba765c386aa62c4d29f80b12` · dirty: `0`

Final Phase 2 arc end — `origin/main` HEAD after PR #24 (`d8e34783`) and PR #25 (`6dcfa353`) landed. `duplicated lane pairs = 0`; every `PAIR_SPECS` row `removed` or `delegated`. `walker families = 4` (aspirational; waived by operator directive per `docs/evidence/PHASE_2_COMPLETION_2026-08-13.md` §10).

```text
═══ PHASE METRICS ═══
tree      : /private/tmp/anubis-main-1499
commit    : 6dcfa35363d84cdeba765c386aa62c4d29f80b12
branch    : HEAD
dirty     : 0 entries

metric                                        value   target
--------------------------------------------------------------------------
middle/mod.rs lines                           28341   strictly decreasing (Phase 2+)
  pair: source walkers                      removed   shared implementation expected
  pair: pattern seeders                     removed   shared implementation expected
  pair: return summaries                    removed   shared implementation expected
  pair: block walkers                     delegated   thin adapter over walk_block_labels
duplicated lane pairs                             0   0
  ^ lines in duplicated pairs                     0   decreasing
source-walker pair similarity               removed   shared implementation expected
fused cross-lane joins                            1   0 (per-lane joins)
  ^ call sites                                   16   -
_ => in label-lane walkers                        0   0 (as in capability.rs)
lane facts with no join                           1   0 (every lane a lattice)
  walk_expr @532 (helper)                top=1 nest=0   dispatcher top must be 0
  walk_expr @2489 (dispatcher)           top=0 nest=1   dispatcher top must be 0
  walk_stmt @2244 (dispatcher)           top=0 nest=2   dispatcher top must be 0
walker families                                   4   non-increasing, → 1
general ExprStmt arm: walk_block_taint          yes   via walk_block_labels
general ExprStmt arm: walk_block_secret         yes   via walk_block_labels

Expr variants: 29   Stmt variants: 15

PHASE_METRICS: OK
```
