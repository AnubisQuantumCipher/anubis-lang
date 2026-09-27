# Failed-guard write: additional frozen controls

Fixture-only commit `60b8d4e429d2f96f54a001e851077c51b1f1c912` adds
three Safe value-position match controls before the proposed state-transfer
repair. They supplement the [valid/invalid fallthrough pair and direct twin](MATCH_GUARD_WRITE_REGISTRATION_2026-09-26.md).
The newly frozen intents are all REJECT: a prior nonmatch fact must refer to
the saved scrutinee rather than `x` after a failed guard writes it; an arm
binder named `x` must not be confused with the enclosing `x`; and a call
inside a guard must be checked even when the guard returns false.

The immutable `e7187a31` checker (SHA-256
`8b85333dc2615b13d2d044cf19d421a2d40e7c5f6d4b63145465ee400516fe2b`)
checked the exact new source bytes before this fixture-only commit. The
source/binary-bound registration manifest SHA-256 is
`43b85d760396c4595c4883988177b6772798a14269a9b066672ea34000831859`.
Both the stale nonmatch and binder-shadow controls were refused as
`ANUBIS_ASSERTION_UNDECIDED`, with an unencoded precondition. The guard-call
control was `ANUBIS_ASSERTION_DISPROVED`. No parser error or silent accept was
observed. These are baseline classifications, not a repaired result.

| Registered source | Intended | Pinned checker |
|---|---|---|
| [`r2arm_guard_write_stale_nonmatch_invalid`](../../tests/soundness/matrix/cases/r2arm_guard_write_stale_nonmatch_invalid.anb) | REJECT | UNDECIDED |
| [`r2arm_guard_write_binder_shadow_invalid`](../../tests/soundness/matrix/cases/r2arm_guard_write_binder_shadow_invalid.anb) | REJECT | UNDECIDED |
| [`r2arm_guard_write_call_invalid`](../../tests/soundness/matrix/cases/r2arm_guard_write_call_invalid.anb) | REJECT | DISPROVED |

Separate contract-free native Safe witnesses use the same control flow and
print the value observed at each executed call, followed by the match
result. The stale-nonmatch and binder-shadow witnesses each printed `0` then
`0`; the guard-call witness printed `0` then `1`. Their retained local
manifest SHA-256 is
`1bf616752fcc0ddd78ba1b5a5eda9cc81534617c2d3c3245e4c8852070e9f271`.
They establish the selected runtime path for those bytes, not a proof that
the verifier models it. Registry/case-set and documentation-drift checks
passed. The provisional full matrix runner still reports `INCOMPLETE`, and
`history.tsv` was not silently rewritten to imply a complete run.
