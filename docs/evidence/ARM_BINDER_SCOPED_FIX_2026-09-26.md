# Match-arm contract binder: scoped local result

This receipt is for commit `70b200c7197f1cee4eb0ceb6429da7b70ed649c5`
on the local `codex/register-round2-20260926` stack. It is a scoped Safe-mode
contract fix, not an Anubis 1.0 or full soundness seal. The preceding
`72e1390b` source-graph primitive is separate and does not grant package
admission.

The prior checker could drop `f(x)`'s violated `requires(x > 0)` inside a
`match` when a sibling arm also bound `x`. The new arm-local solver scope
retains that obligation, models exact scalar payloads where justified, and
restores outer binding facts after the arm. Exact dead guards do not create
body obligations; a later arm behind an unmodeled failed guard receives a
typed reachability uncertainty rather than a purported checked disproof.
The security walkers still use their existing distinct policies.

## Source-bound local comparison

The lead built one clean AArch64 Linux release CLI from the named commit with
`cargo build --release -p anubis`, rustc
`1.97.0-nightly (82bee9650 2026-05-09)`. Its SHA-256 is
`85dcee4d3da557d4a66931971a15c772e79a54dbff2227ba7c496748ded9f32d`.
The local binary/build manifest SHA-256 is
`fdf4b32b9f8369589a23755423c2b262e204c736c8eb56a9a43797bb1792d641`;
the frozen selected-control manifest SHA-256 is
`a7ffa86bea71e7372f75c2cc07feb7ec21710c5327edbfdbf09cf95736efe049`.
These identify retained local artifacts, not an independent rebuild or a
release signature. Baseline comparisons used the earlier source-bound CLI at
`8fadf5195b877ed6d41f7a4093a43235b1cfcaf0`, SHA-256
`158f7d962cd9f2379452b462617386f37501c5b5eed0e0ac46836860e224bf5a`.

| Required Safe control | Baseline diagnostic | Candidate diagnostic |
|---|---|---|
| [`r2arm_sibling_binder_drops_requires`](../../tests/soundness/matrix/cases/r2arm_sibling_binder_drops_requires.anb) (violated) | PASS, no obligation | DISPROVED |
| [`r2arm_same_binder_valid_match`](../../tests/soundness/matrix/cases/r2arm_same_binder_valid_match.anb) | UNDECIDED | PASS |
| [`r2arm_same_binder_valid_iflet`](../../tests/soundness/matrix/cases/r2arm_same_binder_valid_iflet.anb) | UNDECIDED | PASS |
| [`r2arm_guard_false_binder_valid`](../../tests/soundness/matrix/cases/r2arm_guard_false_binder_valid.anb) | UNDECIDED | PASS |
| [`r2arm_partial_guard_live_invalid`](../../tests/soundness/matrix/cases/r2arm_partial_guard_live_invalid.anb) (violated) | DISPROVED | DISPROVED |
| [`r2arm_partial_guard_dead_valid`](../../tests/soundness/matrix/cases/r2arm_partial_guard_dead_valid.anb) | PASS | PASS |
| Frozen exact-true and exact-false guarded dead-arm controls | UNDECIDED | PASS |
| Frozen valid `Some(1)` against `Some(2)` dead-payload control | DISPROVED | UNDECIDED |

The last row is still a valid-program precision failure. The selected receipt
is therefore **INCOMPLETE**, and no complete match-arm precision or full-matrix
claim follows. The enum-payload mismatch is the next separate implementation
unit. The provisional [matrix runner](../../tests/soundness/matrix/run.sh)
also remains `INCOMPLETE` until reviewed canonical diagnostic expectations
and history publication exist.

The focused arm-binder Safe test passed on the reviewed candidate, as did
`cargo fmt --all --check`, workspace Clippy with `-D warnings`, and the
documentation-drift gate. Independent review of the exact arm diff SHA-256
`d87d0aa92f89b37e73f179fd49d9d0dd62376a18eea4846c7d9501e8a47acd53`
found no patch-introduced blocker and explicitly retained the enum precision
gap. Those focused checks do not replace the required final workspace suite.

An attempted unfiltered compiler-library suite was stopped with exit code
130 after discovering it had already executed a Research-mode test in this
Linux guest, outside the repository's required disposable Tart environment.
Its complete log is retained with the local manifest and **is not a gate
witness**. No Research, crash, fuzz, exploit, hosted, or disposable-guest
result is claimed here. The required suite and platform witnesses remain open.
