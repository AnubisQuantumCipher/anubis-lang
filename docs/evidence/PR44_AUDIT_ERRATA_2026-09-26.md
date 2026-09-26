# PR 44 historical-claim erratum

This forward-only correction compares committed source, receipts, and hosted logs
with the claims made in [PR 44](https://github.com/AnubisQuantumCipher/anubis-lang/pull/44). A read-only audit
was independently checked against Git history; no historical pin was rerun for
these corrections. A dated measurement remains tied to its original source and
workload. This erratum does not rewrite a published commit or turn an unrun gate
into a pass.

| Claim in the old record | Correction and primary evidence |
|---|---|
| [c0f5b884](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c0f5b884d5c8bedd0c27255062701552e8e01b39) changed two dated W1 language rows to `271/271`. | The source-matched July 29 W1 row in [the original `9a03c2b6` record](https://github.com/AnubisQuantumCipher/anubis-lang/commit/9a03c2b6) reports `252/252`. The other W1 table in [the original `2160f4a4` record](https://github.com/AnubisQuantumCipher/anubis-lang/commit/2160f4a4) reports `253/253`, but its exact run measurement is unrecovered: [the Phase-1 start receipt](PHASE_1_COMPLETION_2026-07-30.md) calls its cited `281e0e…` pin a pre-change comparison instrument. The later `259/259` inventory is not either W1 measurement. `docs/CLAIMS.md` now records this distinction; no historical pin was rerun. |
| [c79f35bd](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c79f35bdee494ecf15892943f7f56c7b2585cc8e) used `161` as the immediate pre-change silent-accept count. | The [c527a48d code receipt](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c527a48db22a9ce873d8c160bd1b3a6dec1042ed) reports `68` for immediate predecessor `anubis-stridx1` on its 489-case matrix; `161` belonged to older `anubis-sortchk12`. |
| [cba96685](https://github.com/AnubisQuantumCipher/anubis-lang/commit/cba966859713897050aae8b33ef40c18b9575d42) gave `173` as the immediate pre-change count and `33 s` as the fan-out baseline. | The [6fe90617 code receipt](https://github.com/AnubisQuantumCipher/anubis-lang/commit/6fe9061742f1e8518cd0a850f4285a9722a75246) reports `25` for immediate predecessor `anubis-lamfn1` on its 509-case matrix; `173` belonged to older `anubis-sortchk12`. That receipt reports the 40-function case as `23 s` to `5.0 s`. Timing was not repeated here. |
| [cea002da](https://github.com/AnubisQuantumCipher/anubis-lang/commit/cea002da49a5d58feb7afa832a2d9a13cba57496) said 537 defects were all matrix cases and fixed. | [0731ecf7](https://github.com/AnubisQuantumCipher/anubis-lang/commit/0731ecf7ed386bd4e5e549149464aaebe84ff8f3) distinguishes 537 findings across uncommitted review versions, 531 cases added to the matrix, and 36 newly documented wrong-class refusals. Those are different populations and the wrong-class cases were not all fixed. |
| [c87ad1cf](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c87ad1cfa8d279435a094304515e9d9edb8ead5e) says 13 new carrier fixtures and changes the docs-drift floor from 53 to 38. | [84296cef](https://github.com/AnubisQuantumCipher/anubis-lang/commit/84296cef66c2d7aadf0c02393bf6d16324a7e29c) added 12 carrier fixture files under `examples/security`. The historical-stamp classifications are audited below. Some have dated support; others hid wrong or unrecoverable measurements. A pass at the lower floor does not validate all exclusions. |
| [3b90cd4b](https://github.com/AnubisQuantumCipher/anubis-lang/commit/3b90cd4b73f83ffe55c93ea2c80804a0b7964001) claimed formatting clean. | The later c527a48d receipt admits an unformatted test, and that commit formats it. The earlier claim was not established. |
| [4bdfcec8](https://github.com/AnubisQuantumCipher/anubis-lang/commit/4bdfcec8f4c27030cbafafad7889dbaea4c9abe0) said G19 passed at `84e72f5a`. | Hosted runs [35971050436](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/35971050436) and [35970973304](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/35970973304) stopped after G18; G19 did not run there. The later [e6588518 hosted run](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36097287313) failed G19 on `push_diag`, which [a4ca63c5](https://github.com/AnubisQuantumCipher/anubis-lang/commit/a4ca63c5e6f53b6c70a3fbad7f9a430c9cdb90dd) repaired. |
| [0275f5f3](https://github.com/AnubisQuantumCipher/anubis-lang/commit/0275f5f38ea2d8d97b7c95df3c94bba8c4e0e66f) cited 1112 matrix cases. | Its committed registry contained 1145 rows after the rebase; [e6588518](https://github.com/AnubisQuantumCipher/anubis-lang/commit/e65885186fc35bc5f01bc7a8455123253da0b920) records the corrected combined-tree count. |
| The [1696925b](https://github.com/AnubisQuantumCipher/anubis-lang/commit/1696925b580b776ac83c7066e73cbb908c9428ec), [f878d41c](https://github.com/AnubisQuantumCipher/anubis-lang/commit/f878d41c636ad3bbed75ccf11a25cf6e8b409e92), and [2f0b2805](https://github.com/AnubisQuantumCipher/anubis-lang/commit/2f0b280531c6368f03066a994df46e37decc94c4) receipts omitted G27. | The walker call shape changed at 1696925b. [Hosted run 36121975757](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36121975757) later failed G27 with `substring not found`; [209b5fe7](https://github.com/AnubisQuantumCipher/anubis-lang/commit/209b5fe7ae4bf6af9ac623238bf17771386f2bbb) repaired the test anchor. Do not infer a G27 pass from the earlier receipts. |
| [d80bc38c](https://github.com/AnubisQuantumCipher/anubis-lang/commit/d80bc38c6967f9067600ba4e2e5ffae2b7c58788) called match/if-let precondition loss independently reproduced. | Its own defect inventory still said “to-reproduce.” [e99db1d1](https://github.com/AnubisQuantumCipher/anubis-lang/commit/e99db1d19338105016a666f63393ebb8732daebe) later recorded the reproduction and fix. The later fixed status is not reversed by this contemporaneous receipt inconsistency. |

[The matrix rename map](../../tests/soundness/matrix/renames.tsv) records the
old and new IDs in commits `8424dc0c`, `c527a48d`, and `9aadfe41`.
Git reported every mapped source as `R100`; the registry intent and category
were unchanged at each rename. This is an identity map, not a reclassification
or a claim that later verdicts were unchanged.

## Documentation-stamp reconciliation

A per-stamp trace of `scripts/lib/docs_drift_scan.py` on the prior tree counted
38 stamps, including the two W1 language rows that `c0f5b884` had changed to
the current fixture count. Those rows are dated measurements and no longer
pretend to be current claims. The corrected scanner counts 37 stamps. Its
named `language-core-inventory` guard now requires the current disk count in
`docs/CLAIMS.md` to exist exactly once and match the derived fixture inventory;
the historic W1 ratios remain exempt. Regression tests also show that putting
“historical” beside a stale current claim cannot hide the stale number. The
floor changes from 38 to 37 for this documented reclassification. No duplicate
live count was added to inflate the floor, and fixture inventory is not called
a fixture-suite PASS.

This correction also removes a misclassified historical stamp from the first failed
July host-seal paragraph. A later edit had called its native-authoritative corpus
`current` while retaining that paragraph's dated run result. The corrected paragraph
attributes the original 921-file inventory to the old narrative, and the distinct
top-of-page current native inventory remains checked by the scanner. The corrected
gate observes 36 stamps with zero drift; the floor moves from 37 to 36 solely for
this historical reclassification. No current inventory claim was removed or duplicated.

The `c87ad1cf` reduction from 53 to 38 reclassified 15 stamps as historical.
The scope-grouped audit of those reclassifications has these outcomes:

| Historical scope | Audit result and controlling record |
|---|---|
| Release-seal snapshot in `AGENTS.md` | Supported as a dated release snapshot by [the Phase-4 receipt](PHASE_4_COMPLETION_2026-08-15.md) and [the completion-status receipt](COMPLETION_PHASES_5_8_STATUS_2026-08-15.md). Its counts are not live worktree claims. |
| Phase-4 candidate security and native rows in `docs/CLAIMS.md` | Supported for that candidate by [the Phase-4 receipt](PHASE_4_COMPLETION_2026-08-15.md); they are not current measurements. |
| July Phase-1 deciding technical epoch | The [July 31 supporting-check table](PHASE_1_COMPLETION_2026-07-31.md) reports language `253/253` and security `327/327`, not the later `259/259` and `337/337` retained by `c87ad1cf`. `docs/CLAIMS.md` now corrects the historical row with provenance. |
| First July 30 failed host seal | Its `out/phase1_host_seal_20260730T133327Z` run artifact was not recovered. The surviving [`2160f4a4` narrative](https://github.com/AnubisQuantumCipher/anubis-lang/commit/2160f4a4) reported `327/327` security and `253/253` language, but the actual measurements for that exact failed run remain unknown here. `337/337` and `259/259` were not justified as historical exemptions. |
| Audited July 30 host seal | Its `out/phase1_host_seal_audited_20260730T154003Z` artifact was not recovered. The original [`2160f4a4` narrative](https://github.com/AnubisQuantumCipher/anubis-lang/commit/2160f4a4) reported 921 native files and 916 tracked files, attributing the gap to five untracked files. The files and run were not independently recovered, and the run remained a HOLD, not a whole-tree seal. A later `0b0889da` edit substituted 937 for 921 without a new measurement of this July run. |
| First W1 table | [`2160f4a4`](https://github.com/AnubisQuantumCipher/anubis-lang/commit/2160f4a4) reported `327/327` security and `253/253` language. The table's cited `281e0e…` pin is described as a pre-change comparison instrument in [the Phase-1 start receipt](PHASE_1_COMPLETION_2026-07-30.md); the exact W1 run measurement is unrecovered. The later `337/337` and `259/259` values must not be substituted. |
| July 29 W1 table | The [original `9a03c2b6` record](https://github.com/AnubisQuantumCipher/anubis-lang/commit/9a03c2b6) reports `327/327` security and `252/252` language for commit `03210603` and the `58ba4abc…` source-matched pin. The later values in `c87ad1cf` and `c0f5b884` were not observations of that run. |
| W1 native rows labelled both current and historical | The later `current corpus 937` wording mixed a later inventory with historical W1 results. `docs/CLAIMS.md` now preserves the original W1 reports and separates them from later inventories. |

This audit corrects the historical record. It does not retroactively validate
`c87ad1cf`'s floor reduction or claim that count-only scanning covers every
current promise. The named live-language
inventory check and scanner regression tests described above remain the current
protection for that specific claim; broader structured claim coverage is still open.
