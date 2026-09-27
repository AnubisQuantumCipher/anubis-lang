# Value-position constructed-enum match: scoped local result

This receipt covers local code commit `1280e21edfb5f89d68d3f088020083bffff4d086`,
following the [pre-fix fixture registration](ARM_ENUM_EXPR_REGISTRATION_2026-09-26.md).
The value-position contract walker now evaluates the scrutinee, skips an arm only
when its exact constructed payload cannot match, evaluates a potentially
reachable guard, skips a body behind a syntactically false guard, and stops
after a definite terminal arm. Unknown payloads retain their obligations.
This is a scoped reachability repair, not a general proof of match analysis.

The lead built a release CLI from the clean code commit on Linux AArch64 with
`rustc 1.97.0-nightly (82bee9650 2026-05-09)`. The build reused the local
Cargo target, so it is not an independent clean-room rebuild. The binary
SHA-256 is `e5362d4a488041ae3ffa4d2abcc5af049e81bbf4462f84b8b62bd0abc98843df`;
the retained local binary/build manifest SHA-256 is
`774ce286127019366047e20ff9de203500574e24f61b117c3f1c27b27ac309ed`.
The selected `r2arm_*` comparison manifest SHA-256 is
`b96cc27b76d92f1595f0b633daf1ca6dd42f65a712a0c549de0ff8adb763f5e8`.
It binds the same registered source bytes and each CLI binary. The baseline
binary is the immutable `f15e1b83` checker, SHA-256
`6f50cd26e84943b5eb08350e9f7212eb129936cd012925efdc4d053ae8595d07`.

| Frozen Safe control | `f15e1b83` | `1280e21e` |
|---|---|---|
| [`r2arm_enum_expr_payload_dead_valid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_payload_dead_valid.anb) | DISPROVED | PASS |
| [`r2arm_enum_expr_payload_live_invalid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_payload_live_invalid.anb) | DISPROVED | DISPROVED |
| [`r2arm_enum_expr_named_dead_valid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_named_dead_valid.anb) | DISPROVED | PASS |
| [`r2arm_enum_expr_named_live_invalid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_named_live_invalid.anb) | DISPROVED | DISPROVED |

The comparison script observed 56 registered `r2arm_*` IDs and 69 binary/form
rows. Only the two dead-valid forms flipped, both from DISPROVED to PASS;
every other selected typed outcome stayed unchanged. The candidate's selected
forms report 34 PASS, 33 DISPROVED, and two UNDECIDED. Those undecided forms
are the newly registered guard-write controls below, not proofs or checked
counterexamples. This selected comparison does not make the provisional full
matrix runner complete; it still reports `INCOMPLETE`.

Ordinary Safe native runs of contract-free branch witnesses on the candidate
binary printed `0` for the dead positional and named arms and `-1` for their
live twins. The local runtime manifest SHA-256 is
`2f6a3007f65d919891c30877ed59831a03edf17b1fb71c2ba2396f8383ffd0b4`.
Those runs establish branch behavior for the witness bytes, not universal
compiler-to-runtime correspondence.

The exact standalone patch SHA-256
`94ccf3f01d48832a7821a6668a4a186fb64326e07ea2315f43056a86e6654a39`
received an independent read-only GO for this scope. The reviewer required
the source-bound verdict comparison and noted that a write-bearing match
guard's later facts remain a separate risk. Lead-owned focused Safe
integration tests passed (four value-match tests and two helper tests), as
did workspace Clippy with `-D warnings`, format, diff, registry/case-set,
and documentation-drift checks. No unfiltered compiler-library, full
workspace, disposable-guest, hosted, independent rebuild, or release gate
is claimed for this code commit.

The [guard-write registration](MATCH_GUARD_WRITE_REGISTRATION_2026-09-26.md)
preserves that separate required valid case. Its valid fallthrough remains
UNDECIDED on this candidate, so match precision is not complete.
