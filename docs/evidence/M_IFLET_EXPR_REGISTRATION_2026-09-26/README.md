# Value-position `if let` contract registration — selected Safe checks

The fixture/registry commit [`f344987dfe9c05ead6d1a4584a165b64e9519f10`](../../../tests/soundness/matrix/registry.tsv)
copies the archived `mi1`, `mi2`, `mi6`, and valid `mi7` sources byte for byte
into the soundness matrix. The independently reviewed staged patch has SHA-256
`ea5c099075414253ee2731f73e1135030412de4292bfd74f51f45933250ae1e8`.
The [review](INDEPENDENT_REVIEW.md) is a path-sanitized copy of the original
read-only review, whose SHA-256 is
`d69fdf2ada90e0fbe230b7e52085107ce51a7d578cbb39a8b591721bb5587416`.
Only the checkout and local-evidence path prefixes were replaced; the retained
copy has SHA-256 `bd139806e583694018cfcc8dbefb3e5830aaf7277834b7103232281c48e81edf`.

The [machine-readable selected results](selected-results.json) bind each source
and raw [stdout/stderr](.) to SHA-256 digests, the CLI binary digest, command
shape, and two distinct revisions: the checked tree before registration and
the registration commit. The CLI was the immutable clean-HEAD compiler artifact
identified in the [captured solver-outcome receipt](../CAPTURED_TOOL_OUTCOME_2026-09-26/README.md).
The checked revision preceded only fixture and documentation changes; this is
a selected checker comparison, not a fresh build of the registration commit.
`git diff` between the binary's code commit and the registration commit lists
no changes under `compiler`, `solver`, `tools/anubis`, `Cargo.lock`,
`rust-toolchain.toml`, or `vendor`; that is a scoped source comparison, not a
complete build-provenance claim for the later tree.
The per-process user-bus environment was supplied to reach the existing
systemd user scope. The retained output files are from the successful scoped
checks.

| Matrix cases | Intended result | Observed current Safe result |
|---|---|---|
| `rc25_mi1_expr_iflet_then_violation`, `rc25_mi6_expr_iflet_in_arg_violation` | Reject reached `f(-1)` precondition violations | Exit 1, typed `ANUBIS_ASSERTION_UNDECIDED`: candidate counterexample has unmodeled branch reachability. |
| `rc25_mi2_expr_iflet_binder_violation` | Reject reached `f(y)` with `y = -5` | Exit 1, typed `ANUBIS_ASSERTION_UNDECIDED`: precondition unencoded; no solver ran for it. |
| `rc25_mi7_expr_iflet_valid_twin` | Accept reached `f(3)` | Exit 0, passing JSON summary. |

The [older reconciliation](../RECONCILE_2026-09-25/RECONCILE.md) reports
native violation witnesses and earlier silent accepts for the three negative
sources; this receipt did not rerun that historical compiler or native
execution. The current negative verdicts are fail-closed but are **not checked
disproofs**. The remaining implementation dependency is to prove reachability
of the constructed `Some` arm, carry its binder value to the obligation, and
retain the valid twin. These selected outcomes do not establish a full-matrix
pass, source-to-obligation correspondence, or product assurance.
