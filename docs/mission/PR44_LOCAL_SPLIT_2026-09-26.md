# PR 44: local review stack preparation

This is an integration artifact, not a second defect registry. The current
soundness and release status remains in [CLAIMS](../CLAIMS.md) and
[DEFECTS](DEFECTS.md). [Draft PR 44](https://github.com/AnubisQuantumCipher/anubis-lang/pull/44)
must not be merged whole. Its remote head observed for this preparation was
`195e33592202337e32be6ca538c45b83a75dd636`; `main` was
`e34d0c89c2181e8bd02ff52b0ab5d50cf97bac4b`.

The lead created these local branch refs at existing immutable commits. Each
unit is a stacked diff against the preceding row's head; the later rows must
target their immediate predecessor branch during review. Nothing here was
pushed, opened as a PR, force-pushed, or merged. Extracted-tip gates and human
review have **not run**. The local machine-readable split manifest SHA-256 is
`11d781a192302da8ff8284933696c848ae4c4ad57163df17fe074804956df46f`.

| Local review branch | Base → head | Diff observed by Git | Review focus |
|---|---|---|---|
| `codex/pr44-01-evidence-20260926` | `e34d0c89` → `b49093d9` | 2 files; +741/−5 | Program-evidence v3 emission and independent published-refutation replay. Source→CNF correspondence remains open. |
| `codex/pr44-02-foundation-20260926` | `b49093d9` → `69755b7b` | 8 files; +647/−16 | Solver work budget, edition refusal, diagnostic hygiene and native fork/exec retry. |
| `codex/pr44-03-diagnostics-20260926` | `69755b7b` → `0b45b7dd` | 11 files; +2133/−53 | JSON diagnostics and witness-scope reporting, with the formatting gate repair. Preserve disproved versus undecided. |
| `codex/pr44-04-budgets-20260926` | `0b45b7dd` → `7220c4b1` | 17 files; +828/−78 | Native gate completion/resource handling and explicit solver-space limits. |
| `codex/pr44-05-item21-carriers-20260926` | `7220c4b1` → `029d5538` | 9 files; +183/−25 | Place-assigned callable and unannotated element/factory return carriers. |
| `codex/pr44-06-item21-contract-20260926` | `029d5538` → `c87ad1cf` | 22 files; +2342/−329 | Family-1 contract carrier, fixtures and receipt. The dated receipt's fixture miscount and stamp exclusions need the existing [forward correction](../evidence/PR44_AUDIT_ERRATA_2026-09-26.md) adapted before this unit is offered for merge. |

Git ancestry checks confirmed each row's base is an ancestor of its head and
each head is an ancestor of the observed PR 44 remote head. The evidence range
`e34d0c89..7220c4b1` has 13 commits, correcting the earlier 12-commit
description in [MISSION](../MISSION.md). The item-21 range contains its
original four commits; no cherry-pick or duplicate implementation was made.

This is the first extraction. Later PR 44 commits, including matrix history,
IFC v2, Linux work, corrections, and current trust-gap fixes still need
separate dependency-correct units. Before opening any extracted PR, run its
required source-tip gates on an isolated branch, check the diff for public
paths and retained historical evidence, prepare its final body with exact
receipts, and obtain independent review. Protected-branch merge remains a
separate authorization boundary.
