# Declared-field rewrite after release — registration receipt, 2026-09-26

Fixture-only commit `66af64194f6aea52ac474fff9bb4a7f3f5ba2845`
(tree `08fa547e7a5c2d415caaad84aa4d9f1c3538fe39`) freezes a matched
Safe-mode pair before a D9 implementation change. The registered sources are
[`d9q_released_public_field_rewrite_valid.anb`](../../../tests/soundness/matrix/cases/d9q_released_public_field_rewrite_valid.anb)
(SHA-256 `8fce3f0f66ac85a8ca904c48a0819b8e5b3e43c35209b3aab8af30c57c698050`)
and
[`d9q_released_secret_field_rewrite_invalid.anb`](../../../tests/soundness/matrix/cases/d9q_released_secret_field_rewrite_invalid.anb)
(SHA-256 `0a4bbe679f4b0d2a6efe21d31f82a3e18a332f8abe6089e4403504100e9f54fc`).
Their [registry](../../../tests/soundness/matrix/registry.tsv) SHA-256 is
`4bd22509344c4fc7259db0f2a94e1aeb8d2e720fa37f3898e84f8f0cc76d4478`.
Independent read-only review approved the paired source syntax and ACCEPT/REJECT
intents before registration.

The pair starts with a `Row` whose `k` field is declared `secret<i64>`, releases
the original value, and then writes a clean literal to either `n` or `k`.
Reading public `n` after its write must remain accepted. The new write to
declared secret `k` must restore protection at the subsequent `println(row.k)`;
release of the earlier value cannot authorize a later protected value. This
tests both the direct source/declared-field producer and the independent IFC
producer. A generic failing aggregate verdict alone cannot establish that the
direct producer retained the obligation.

The [source-bound baseline](baseline.json) uses code commit
`fc72526b5477f6475d33adf3d86ad02a467b1046` (tree
`7c671193777e5ee6f7f646b6fc9fc013dde01463`) and its local Linux
AArch64 release binary SHA-256
`98e8090a07716e4876c65422c368e373d92717456edc235f296d8b687f17aac0`.
The JSON SHA-256 is
`97e6da843022e99c1f9ff437f8bfecb4c2f63cede9ae66c564e59cd17f2cceb1`;
it records source and raw-output hashes, exit codes, typed diagnostics, and
summary verdicts. The [unmodified stdout/stderr logs](logs/) are included.
This is a SHA-identified local build, not an independent rebuild or a
published immutable binary pin.

| Registered case | Intent | Baseline result | Required candidate result |
|---|---|---|---|
| `d9q_released_public_field_rewrite_valid` | ACCEPT | exit 0, `pass`, no diagnostics | accepted for the public projection |
| `d9q_released_secret_field_rewrite_invalid` | REJECT | exit 1, `fail`, direct declared-field `ANUBIS_SECRET_EXFILTRATION` and an IFC2 `ANUBIS_SECRET_EXFILTRATION` | refused at the protected read/egress by the direct declared-field producer as well as IFC2 |

The fake-checker matrix harness passed (`Ran 48 tests`, `OK`); the local
log SHA-256 is
`13f30739c15f6d4a9b933b079aaed311f849407d0fc046765b133fab373df280`.
The first documentation-drift attempt failed closed because this evidence
directory was created during the manifest's repeated source-tree scan. Its
retained local log SHA-256 is
`22ae0df85d2e576e7a7cf78da3f8c25d499db85bd38ac12b5b646623e4bae10e`.
After the source tree stopped changing, the gate passed (`DOCS_DRIFT_GATE:
PASS`, zero drift); its log SHA-256 is
`e8ab1076e7a5ebf0f1376f2beaf16e8c08fc80aaab0141d69917ed1e94801ed7`.
The D9 v3 scratch implementation was held during independent static review:
its shape transfer could mark `row.k` clean after the post-release write and
drop the direct obligation, even though IFC2 might still refuse the program.
That patch was never applied to the branch. No D9 repair, full matrix,
whole-workspace, guest, hosted or release result is claimed here.
