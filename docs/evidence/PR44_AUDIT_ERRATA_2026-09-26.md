# PR 44 historical-claim erratum

This forward-only correction compares committed source, receipts, and hosted logs
with the claims made in PR 44. The [read-only audit](https://github.com/AnubisQuantumCipher/anubis-lang/pull/44)
was independently checked against Git history; no historical pin was rerun for
these corrections. A dated measurement remains tied to its original source and
workload. This erratum does not rewrite a published commit or turn an unrun gate
into a pass.

| Claim in the old record | Correction and primary evidence |
|---|---|
| [c0f5b884](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c0f5b884d5c8bedd0c27255062701552e8e01b39) changed two dated W1 language rows to `271/271`. | Those W1 rows measured `259/259`; their original values are restored in `docs/CLAIMS.md`. The current language inventory is a separate count and is not a W1 test result. |
| [c79f35bd](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c79f35bdee494ecf15892943f7f56c7b2585cc8e) used `161` as the immediate pre-change silent-accept count. | The [c527a48d code receipt](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c527a48db22a9ce873d8c160bd1b3a6dec1042ed) reports `68` for immediate predecessor `anubis-stridx1` on its 489-case matrix; `161` belonged to older `anubis-sortchk12`. |
| [cba96685](https://github.com/AnubisQuantumCipher/anubis-lang/commit/cba966859713897050aae8b33ef40c18b9575d42) gave `173` as the immediate pre-change count and `33 s` as the fan-out baseline. | The [6fe90617 code receipt](https://github.com/AnubisQuantumCipher/anubis-lang/commit/6fe9061742f1e8518cd0a850f4285a9722a75246) reports `25` for immediate predecessor `anubis-lamfn1` on its 509-case matrix; `173` belonged to older `anubis-sortchk12`. That receipt reports the 40-function case as `23 s` to `5.0 s`. Timing was not repeated here. |
| [cea002da](https://github.com/AnubisQuantumCipher/anubis-lang/commit/cea002da49a5d58feb7afa832a2d9a13cba57496) said 537 defects were all matrix cases and fixed. | [0731ecf7](https://github.com/AnubisQuantumCipher/anubis-lang/commit/0731ecf7ed386bd4e5e549149464aaebe84ff8f3) distinguishes 537 findings across uncommitted review versions, 531 cases added to the matrix, and 36 newly documented wrong-class refusals. Those are different populations and the wrong-class cases were not all fixed. |
| [c87ad1cf](https://github.com/AnubisQuantumCipher/anubis-lang/commit/c87ad1cfa8d279435a094304515e9d9edb8ead5e) says 13 new carrier fixtures and changes the docs-drift floor from 53 to 38. | [84296cef](https://github.com/AnubisQuantumCipher/anubis-lang/commit/84296cef66c2d7aadf0c02393bf6d16324a7e29c) added 12 carrier fixture files under `examples/security`. The floor reduction is recorded as a coverage change still requiring item-by-item classification review; a pass at floor 38 does not validate the excluded stamps. |
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

The earlier `c87ad1cf` reduction from 53 to 38 remains an open, separate
item-by-item exemption audit. This correction does not authorize all exclusions
made then or prove the completeness of count-only coverage.
