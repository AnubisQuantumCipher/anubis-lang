# Match guard write and fallthrough: frozen controls

Fixture-only commit `9ca57801eb0c0e33467e68d74fbd253de3b5e7c7`
registered a Safe valid/invalid fallthrough pair and the direct violated
call. A failed guard assigns outer `x` before the next arm calls a function
requiring a positive argument. In the valid case it assigns `1`; in the
invalid case it assigns `0`. The direct `f(0)` twin establishes that the
ordinary direct-call obligation is checked.

The immutable `f15e1b83` checker binary (SHA-256
`6f50cd26e84943b5eb08350e9f7212eb129936cd012925efdc4d053ae8595d07`)
refused both fallthrough cases as `ANUBIS_ASSERTION_UNDECIDED` with an
explicit unencoded `requires`; it DISPROVED the direct twin. The later
[value-position candidate](ARM_ENUM_EXPR_PRECISION_2026-09-26.md) retained
those typed outcomes. Thus the invalid fallthrough was **not** a reproduced
silent accept on these pins, and the valid fallthrough is an open precision
defect. The local registration manifest SHA-256 is
`a153b88e308bdd1231b0e3bb9dac4dfb613e9f73c7b58fc52ca12bac13a8f90c`;
it binds every fixture byte, the earlier checker binary, exit code,
diagnostic class, and retained logs. The earlier binary predates this
fixture-only commit; no build from the registration commit is implied.

| Frozen control | Intended | Pinned checker |
|---|---|---|
| [`r2arm_guard_write_failed_valid`](../../tests/soundness/matrix/cases/r2arm_guard_write_failed_valid.anb) | ACCEPT | UNDECIDED |
| [`r2arm_guard_write_failed_invalid`](../../tests/soundness/matrix/cases/r2arm_guard_write_failed_invalid.anb) | REJECT, checked if modelable | UNDECIDED |
| [`r2arm_guard_write_failed_invalid.direct`](../../tests/soundness/matrix/cases/r2arm_guard_write_failed_invalid.direct.anb) | DISPROVED | DISPROVED |

Contract-free native Safe witnesses with the same control flow printed `1`
for the valid fallthrough and `0` for the invalid fallthrough. Those runs
observe execution only; removing the contract makes them no verifier proof.
The registry/case-set check and documentation-drift gate passed after
registration. `history.tsv` remains historical: its provisional runner
does not safely publish new history or provide a complete diagnostic oracle.
The source- and binary-bound local manifest is retained until that gap is
closed.
