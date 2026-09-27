# D9 public-field write under secret program counter — registration, 2026-09-26

Fixture-only commit `15bb53a55ce369128853f5012b38b6c210f0ff14`
(tree `a6cd2c43cdff0eb6a5c227e3085166a81a33fe8d`) freezes the
independently reviewed Safe-mode pair
[`d9q_public_pc_public_sibling_valid.anb`](../../../tests/soundness/matrix/cases/d9q_public_pc_public_sibling_valid.anb)
(SHA-256 `e6feedb919460a12097679dfe907f25f957f24575834bf066ec43c00c7da213e`)
and
[`d9q_secret_pc_public_sibling_invalid.anb`](../../../tests/soundness/matrix/cases/d9q_secret_pc_public_sibling_invalid.anb)
(SHA-256 `1be6593f7b04f6819c75aeafeaac7904d9f18ada543f299e79ffcbf3c1375e94`).
The [registry](../../../tests/soundness/matrix/registry.tsv) SHA-256 is
`36f1c1ecc0ee112f1d96e2dae4f8fda3254f70aa3599b51ac5f8e0b1c6d7d7fa`.
Both programs pass a variable `secret<i64>` parameter into `Row.k`, then
write public literals to `Row.n` in both branches and print only `n`.
The invalid guard depends on that secret parameter; the valid guard depends
on an independent public parameter. Rewriting the public field under secret
control must remain a confidentiality obligation even though `k` already
labels the whole row. The public-guard twin must remain usable.

The [baseline](baseline.json) uses compiler code commit
`fc72526b5477f6475d33adf3d86ad02a467b1046` (tree
`7c671193777e5ee6f7f646b6fc9fc013dde01463`) and its local Linux
AArch64 release binary SHA-256
`98e8090a07716e4876c65422c368e373d92717456edc235f296d8b687f17aac0`.
The JSON SHA-256 is
`40671e670e2338ab829909e574e8989e0aa30160e550db59bd5c2288c62d9a18`;
it records exact source and [raw output](logs/) hashes, exit codes, typed
diagnostic classes, and summary verdicts. These are SHA-identified local
bytes, not an independent rebuild or a published immutable binary pin.

| Frozen case | Intent | Baseline Safe `check` | Candidate requirement |
|---|---|---|---|
| `d9q_public_pc_public_sibling_valid` | ACCEPT | exit 1, `fail`, direct root `ANUBIS_SECRET_EXFILTRATION` | accepted for its public guard and public projection |
| `d9q_secret_pc_public_sibling_invalid` | REJECT | exit 1, `fail`, generic direct root `ANUBIS_SECRET_EXFILTRATION` and IFC2 `ANUBIS_SECRET_EXFILTRATION`; no `ANUBIS_IMPLICIT_FLOW` | a direct implicit-flow or equivalent typed confidentiality finding for the secret-dependent public-field write, with IFC2 refusal retained |

The baseline refuses the invalid complete program, so this is not an
observed aggregate Safe false accept. The direct root finding also refuses
the valid twin and does not identify the secret-controlled write. During
independent static review of the unintegrated D9 v4 scratch patch (SHA-256
`741c04e6cf6be6d76ebd58293bee03eea0d43906c5e1fd8032af02b77adbf06a`),
the reviewer traced a direct-producer regression: a clean field shape could
make `expr_source(FieldAccess)` return before considering a secret program
counter, while the existing implicit-flow gate skipped an already-secret
root. IFC2 appeared to retain its separate refusal, but no candidate binary
was built or run. V4 remains withheld; a revised transfer must be independently
reviewed and graded on this pair and the earlier D9 controls.

The fake-checker matrix harness passed (`Ran 48 tests`, `OK`); its local log
SHA-256 is
`06ca98ef6828a10000e523400adb67506d7b0bccc5a70ad6cffd68285872c53b`.
Documentation drift passed after fixture registration with zero drift; its
local log SHA-256 is
`821decb0c8472c8c77117e7078947b4f8d959061a7c52fe0e06a7c2173e6005c`.
No D9 repair, full matrix, whole-workspace, hosted, guest or release result
is claimed by this registration.
