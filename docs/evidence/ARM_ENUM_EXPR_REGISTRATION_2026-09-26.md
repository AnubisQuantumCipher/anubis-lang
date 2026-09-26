# Value-position enum match: precision controls frozen before repair

Fixture-only commit `c6955cd45fd92c16a56e129697b012c2b40ff051`
registers positional and named constructed-enum value-position `match`
controls. The valid cases place `f(-1)` behind a payload literal that cannot
match; the invalid twins use a matching payload and reach the same violated
`requires(x > 0)`. This is a distinct expression traversal from the earlier
[statement-match repair](ARM_ENUM_PAYLOAD_PRECISION_2026-09-26.md).

The immutable source-bound checker binaries at `8fadf519`, `70b200c7`, and
`f15e1b83` each report `ANUBIS_ASSERTION_DISPROVED` for all four frozen
inputs. For the two valid cases, that is an incorrect counterexample to an
unreachable call, not successful security rejection. For the reachable twins,
it is the intended checked rejection. The exact input bytes, binary digests,
exit codes, diagnostics and logs are retained in a local registration manifest
with SHA-256
`04dfdf4806632285bd2c33b8bcdb917c65013ce713b317289a93bdee8744912d`.
The `f15e1b83` binary predates the fixture-only commit but its bytes are
immutable and the new fixture bytes are individually hashed; this is a pinned
pre-fix comparison, not a new binary build for `c6955cd4`.

| Registered control | Intended outcome | Frozen checker outcome |
|---|---|---|
| [`r2arm_enum_expr_payload_dead_valid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_payload_dead_valid.anb) | ACCEPT | DISPROVED |
| [`r2arm_enum_expr_payload_live_invalid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_payload_live_invalid.anb) | DISPROVED | DISPROVED |
| [`r2arm_enum_expr_named_dead_valid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_named_dead_valid.anb) | ACCEPT | DISPROVED |
| [`r2arm_enum_expr_named_live_invalid`](../../tests/soundness/matrix/cases/r2arm_enum_expr_named_live_invalid.anb) | DISPROVED | DISPROVED |

Separate ordinary Safe native witnesses remove only the contract declaration
to observe branch behavior: the dead positional and named forms print `0`,
and their live twins print `-1`. Their local manifest SHA-256 is
`e86b736ba595680612374ffd68a53fa6049324303b4bd9e0fa46f09d070cda8b`.
These are executable branch witnesses, not proof that the checker models
the source correctly. The matrix registry/case-set check and documentation-
drift gate passed after registration. No implementation repair or full-matrix
gate is claimed here; the provisional matrix runner still reports
`INCOMPLETE`.
