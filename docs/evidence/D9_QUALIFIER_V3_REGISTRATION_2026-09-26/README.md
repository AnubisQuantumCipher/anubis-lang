# Additional D9 qualifier witnesses — registration receipt, 2026-09-26

Fixture-only commit `7a525f09dba6449318626c3025fa3b79b75e7ff3`
(tree `2edeaefc7cc160de0bb155a6c5399c873c76f906`) registers four
immutable `d9q_v3_*` Safe sources in `tests/soundness/matrix/cases/` and
their ACCEPT/REJECT intents in `registry.tsv` (SHA-256
`40b11f3500eb7426e6d05297769fc67dc622b47c93be9ec7250105c80e640096`).
The names identify this witness registration, not a language edition or a
completed repair. Independent static review gave GO for the source syntax
and intended outcomes before registration.

The sources were frozen and checked before any D9 candidate was integrated.
The baseline uses code commit `fc72526b5477f6475d33adf3d86ad02a467b1046`
(tree `7c671193777e5ee6f7f646b6fc9fc013dde01463`) and its local Linux
AArch64 release binary SHA-256
`98e8090a07716e4876c65422c368e373d92717456edc235f296d8b687f17aac0`.
Each source was passed to `anubis check --message-format=json` under the
local bounded wrapper. [baseline.json](baseline.json) records exact source
and output hashes, exit codes, typed diagnostics, and summary verdicts; its
SHA-256 is
`4e7996840252091b926841ff037a89909c6f6562337b8a8a9dfee427e17d8295`.
The small, unmodified [stdout/stderr logs](logs/) are checked by those hashes.
No Research or native execution was run for these cases.

| Frozen case | Intent | Baseline check | Diagnostic class |
|---|---|---|---|
| `d9q_v3_factory_public_field_valid` | ACCEPT | pass | none |
| `d9q_v3_factory_secret_field_invalid` | REJECT | fail | `ANUBIS_SECRET_EXFILTRATION` at `println` |
| `d9q_v3_nested_clean_public_write_valid` | ACCEPT | fail | `ANUBIS_SECRET_EXFILTRATION` at `println`; precision defect |
| `d9q_v3_nested_secret_public_write_invalid` | REJECT | fail | `ANUBIS_SECRET_EXFILTRATION` at `println` |

The same SHA-256-identified local release binary also checked every registered `d9q_*` source after
fixture registration. [baseline-all-d9q.json](baseline-all-d9q.json) preserves
their source hashes, exits, typed classes and raw-output hashes for a
same-source candidate comparison; its SHA-256 is
`973b3ba42ff3e448f2bdc4dd6ee58d6503fbed88ca8d8727c6b35340a61d119e`.
The all-D9 raw logs are retained locally outside the repository. The
versioned JSON and source/binary identities make the result auditable while
that local binary is retained; independent rebuilding remains open. The full
original corpus was not run.

The factory pair checks that a source-clean call result cannot erase the
declared protected `k` field while its public `n` sibling remains usable.
The nested-write pair checks a value-position branch after the row is already
root-labelled by `k`: writing a public literal to `n` must remain usable,
while writing a secret there must not leave a stale clean field shape. The
negative baseline refusals identify the egress in the human message, but the
JSON diagnostics have no source location yet; final candidate grading must
preserve the typed class and verify that the intended read/egress is reached,
not count a parse, type, write-site, tool, or unrelated refusal as success.

Independent review held the frozen D9 v2 implementation patch before it
touched this branch. The reviewer found a possible false accept for an
uncertified struct-returning expression marked clean, and a possible stale
field shape after nested writes. These are source-level findings, not runtime
witnesses. A revised implementation must preserve the two negative controls
and accept both valid controls for the right reason, alongside the earlier
registered D9 cases.

The fake-checker matrix harness passed (`Ran 48 tests`, `OK`); its retained
local log SHA-256 is
`fc59ad0d31876942ccb4a2150cdfbcd15ca85efbafadb2dc7280d10bcdc3bebf`.
The canonical documentation-drift gate passed with zero drift after fixture
registration; its log SHA-256 is
`d94e9d5528c8050b2f92c8721e26860037607eeee3f204eea4af7148198a6dd8`.
The full matrix remains `INCOMPLETE`: there is no checked-in live canonical
diagnostic manifest, no complete producer locators, and no final source-to-
binary binding for the runner. This registration is not a whole-corpus or
product-soundness result.
