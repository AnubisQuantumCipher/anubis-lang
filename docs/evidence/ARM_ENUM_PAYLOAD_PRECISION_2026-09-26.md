# Constructed-enum dead-arm precision: scoped local result

This receipt covers local code commit `f15e1b83627379133ce5c94194b156a5d006aeea`.
It extends the preceding [arm-binder result](ARM_BINDER_SCOPED_FIX_2026-09-26.md)
with exact positional and named payload comparisons for a constructed enum in
statement `match` and `if let`. A provably mismatched arm no longer contributes
an unreachable violated precondition; an unknown payload remains reachable.
The comparison follows the native pattern evaluator's positional lookup,
first matching named field, and missing-field `Int(0)` behavior. It does not
cover all value-position `match` precision or establish a general match proof.

The lead built `anubis` in release mode from a clean worktree at this commit
on Linux AArch64 with rustc `1.97.0-nightly (82bee9650 2026-05-09)`.
The build reused the local Cargo target; it was not an independent clean-room
rebuild. The executable SHA-256 is
`6f50cd26e84943b5eb08350e9f7212eb129936cd012925efdc4d053ae8595d07`;
the local binary manifest SHA-256 is
`32e33d818fe109e5aff3b6dfcae68ed7c015c29cc61ef0a0793bf3cc18070d1d`.
The frozen selected-case manifest SHA-256 is
`bd62c0b0c116bb46c163a39a46f96a44eee9defdbb5e8215e2d4d7587fe71081`.
These local artifacts bind the tested bytes and inputs; they are not a release
signature or a hosted/guest gate.

| Frozen Safe control | `8fadf519` | `70b200c7` | `f15e1b83` |
|---|---|---|---|
| [`r2arm_enum_payload_dead_valid`](../../tests/soundness/matrix/cases/r2arm_enum_payload_dead_valid.anb) | DISPROVED | UNDECIDED | PASS |
| [`r2arm_enum_payload_live_invalid`](../../tests/soundness/matrix/cases/r2arm_enum_payload_live_invalid.anb) | DISPROVED | UNDECIDED | DISPROVED |
| [`r2arm_enum_scrutinee_call_invalid`](../../tests/soundness/matrix/cases/r2arm_enum_scrutinee_call_invalid.anb) | DISPROVED | DISPROVED | DISPROVED |
| [`r2arm_enum_named_dead_valid`](../../tests/soundness/matrix/cases/r2arm_enum_named_dead_valid.anb) | DISPROVED | UNDECIDED | PASS |
| [`r2arm_enum_named_live_invalid`](../../tests/soundness/matrix/cases/r2arm_enum_named_live_invalid.anb) | DISPROVED | UNDECIDED | DISPROVED |

For the frozen `r2arm_*` subset, the source-bound comparison checked 50
registered IDs and 124 binary/form rows. Relative to `70b200c7`, the only
typed verdict flips are the two valid dead-arm controls from UNDECIDED to
PASS and their two reachable violated twins from UNDECIDED to DISPROVED. At
`f15e1b83`, the selected subset has 32 ACCEPT/PASS and 30
REJECT/DISPROVED form outcomes. The separate earlier-baseline comparison has
SHA-256 `cd558d01d559b06ea381983551e5bcc39c7a3c3643c048be819e7fa2cc471848`.
This is a selected, provisional comparison: the full matrix runner still
reports `INCOMPLETE`, and the remaining families have not been graded by it.
The registered M-CALL-POSITION controls in this subset now produce their
intended typed outcomes, but unregistered forms and broader expression
positions remain outside this receipt.

Ordinary Safe native witnesses using the same source-bound CLI printed `OK`
for dead positional and named payload arms and `LIVE` for their reachable
twins. Their local manifest SHA-256 is
`cf98d87bf70c78b1d7140ef8cde32643d633b933669c30723eb84baef9ee7a1b`.
These witnesses check executable branch selection, not the full compiler-to-
verification-condition correspondence.

The final diff SHA-256
`412ab95513084df38faaac206d9f9260d0fc439ae82a9efdd9958b48f545cb78`
received an independent read-only GO for this scoped mechanism. Focused
Safe compiler tests for enum reachability, statement-match positions, and
the prior arm-binder regression passed; workspace Clippy with `-D warnings`,
format, diff, registry/case-set, and documentation-drift checks passed. The
retained local validation-log manifest SHA-256 is
`85f9fcd67e3c8a563ba18f197ae5beca4ea082770fab11356bf31214cbe876ee`.
The previously interrupted unfiltered compiler-library run is not reused
here. No final workspace, disposable-guest, independent rebuild, platform,
hosted, or release gate is claimed.
