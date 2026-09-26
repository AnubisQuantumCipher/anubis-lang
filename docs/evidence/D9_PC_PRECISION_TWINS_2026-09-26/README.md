# D9 control-flow precision witnesses — registration, 2026-09-26

Fixture-only commit `e6b4fb7e86286c6314595284c0e014ca6bd49813`
(tree `544db1cb975504b7ee9097aade8be3ae27c0463f`) registers the
`d9q_pc_*` sources and their intended outcomes in the
[matrix registry](../../../tests/soundness/matrix/registry.tsv), SHA-256
`cc0446dd52512be0db111fa91ecebdb5b82d5f70dbab235a7cdb4d3688c9186e`.
These are scoped output-flow controls for loop and pattern binders, lexical
shadowing, closure application, secret loop bounds, and literal-unreachable
match/if arms. The secret-bound loop's timing and resource behavior is outside
the stated output-flow oracle. The [independent source review](INDEPENDENT_STATIC_REVIEW.md)
(SHA-256 `4938d376f69d9826193be25db036d4c21000eba35c509bcf9187c2ca057d346c`)
approved registration and identified the Row-root confound below. It did not
run a checker or a source program.

The [baseline JSON](baseline-all-d9.json), SHA-256
`358ab328a2c6031f8e6538111c9a92b7afced069cb7d2d5d975e2b890990f4bd`,
records source hashes, the registry hash, exit codes, typed diagnostics,
summary verdicts, and raw-output hashes for every registered
`D9-QUALIFIER-PRECISION` source at this fixture revision. The
[new-case stdout/stderr](logs/) are retained byte-for-byte; their hashes are
in that JSON. No output required redaction. The Safe `check` used the local
Linux AArch64 release binary SHA-256
`98e8090a07716e4876c65422c368e373d92717456edc235f296d8b687f17aac0`
from compiler code commit `fc72526b5477f6475d33adf3d86ad02a467b1046`.
The tested `compiler/src/middle/mod.rs` has SHA-256
`494a7752d946158411664bb15c09506033b8bfbf6a9c6dc436f171e368b71b2d`.
The later fixture and captured-bridge commits did not produce this binary;
this is a selected-source baseline, not a rebuild of the current full tree or
a published immutable pin.

| Pair or control | Baseline Safe `check` | What a candidate must establish |
|---|---|---|
| For inner guard local / outer | valid refused `ANUBIS_IMPLICIT_FLOW`; invalid refused direct implicit flow and IFC2 | Preserve the invalid write finding and accept the loop-local write. |
| While-let binder / outer | valid refused `ANUBIS_IMPLICIT_FLOW`; invalid refused direct implicit flow and IFC2 | Distinguish the pattern binder from the outer printed value. |
| Protected shadow / outer | valid refused `ANUBIS_IMPLICIT_FLOW`; invalid refused direct implicit flow and IFC2 | Distinguish lexical bindings and their declared protection. |
| Uncalled / called closure | valid passed; invalid refused direct implicit flow and IFC2 | Do not charge an uncalled body as executed; retain the reached call's flow. |
| Secret-bound For local / outer | valid passed; invalid refused direct implicit flow and IFC2 | Preserve local-write acceptance and secret-bound outer-write refusal. |
| Literal-unreachable / reachable match | valid passed; invalid refused generic direct root egress and IFC2 | Syntax/type-check the dead arm while excluding its write from reachable PC effects; establish the actual secret-PC write producer separately. |
| Literal-false / true inner if | valid passed; invalid refused generic direct root egress and IFC2 | Preserve dead-branch precision; establish the actual secret-PC write producer separately. |

All checked sources produced a final `anubis-diagnostics/1` summary; no parse
error, protocol failure, or timeout was observed. The valid For inner-guard,
WhileLet binder, and protected-shadow cases are **existing baseline precision
defects**, not regressions caused by an unintegrated patch. A plain REJECT on
the two reachable Row cases does not prove detection of the secret-controlled
public-field write: existing whole-Row labeling can refuse `println(row.n)`
even when that write is missed. Credit the PC producer only with a direct
`ANUBIS_IMPLICIT_FLOW` or equivalent writer-located diagnostic, alongside
the IFC2 result. Their valid twins print a constant and therefore test
reachability precision, not public-field projection precision.

The fake-checker matrix harness passed (`Ran 48 tests`, `OK`); its
[retained log](matrix-harness.log) has SHA-256
`04e83a3544c35a5362317482a82e59624e00e020a276d697bbd747f3e0e5ac24`.
The canonical documentation-drift gate passed with `36` stamps and zero
drift; its [machine-readable report](docs-drift-report.json) is retained.
The [gate receipt](gate-receipt.json) binds these local artifacts to the
fixture commit and names the checks that were not run.
These gates validate registration infrastructure and documentation inventory,
not the D9 mechanism.

The [independent v5 candidate review](D9_V5_HOLD_REVIEW.md), SHA-256
`e1ea945eb85f4a7a51868aaeac842a0bcdd8b9c20260bb245ac906888193e88a`,
held scratch patch SHA-256
`20c60a5b21700d685c581c6996e8de990c9bf6a6bf6f25e4bcab4a4e34e8203e`.
It found loop/pattern binder, protected-shadow, deferred closure, and nested
reachability precision risks. No v5 compiler binary was built or run, and no
D9 implementation patch is integrated by this registration. The next
dependency is independent review of a revised typed transfer, followed by
lead-only source-matched Safe checks against all registered D9 cases and
both direct and IFC2 diagnostic classes. The full matrix remains
`INCOMPLETE`; no whole-workspace, hosted, guest, release, or full trust-chain
result is claimed.
