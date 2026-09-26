# D9 branch-local binding sites — scoped Safe evidence (2026-09-26)

Fixture commit `253dd4e3979d1846b584062acc2e3d28a059ad45` freezes the
renamed protected-shadow positive and three negative controls in the
[soundness matrix](../../../tests/soundness/matrix/registry.tsv). Code commit
`e4bfe3001df96e2c1799710cf16532c4d57e4df7` changes the existing direct
implicit-flow producer for Safe `if` branches. For a deliberately closed,
straight-line grammar it resolves each direct assignment to a source-ordered
branch-local `let` or to a pre-branch binding. It removes an assigned root from
the direct refusal only when every write to that root belongs to an explicitly
protected branch-local declaration. Calls, output, nested control flow,
expression-position writes, and root-set disagreement retain the previous
refusal. The other program-counter producers and IFC v2 are unchanged.

The [machine receipt](gate-receipt.json) binds both CLI hashes, the code and
fixture commits, the full [prepatch](prepatch.json) and
[candidate](candidate.json) case results, the [differential](differential.json),
and the complete [prepatch](raw-prepatch.json) and
[candidate](raw-candidate.json) JSON streams and exits. The raw streams were
copied from the lead's capped Safe checks and checked against the per-case
hashes. The [independent applied-diff and evidence review](INDEPENDENT_APPLIED_REVIEW.md)
also checked every source hash and raw output. The review approved candidate
validation and corroborated this selected differential; it is not a broad
integration, D9-completion, or release sign-off.
The path-portable [consistency checker](verify_raw_evidence.py) rechecks the
recorded case bytes, source hashes, typed diagnostics, frozen outcome oracles,
and artifact hashes; it exits nonzero on a mismatch. It does not authenticate
the compiler binary, independently authenticate the receipt against a trusted
publisher identity, or prove the compiler's semantics. Its starting audit was
written by the [independent documentation reviewer](INDEPENDENT_DOCS_REVIEW.md);
the lead added portable paths, explicit frozen oracles, and failing exits.

On the same registered source set, the prepatch CLI refused both protected
shadow positives with `ANUBIS_IMPLICIT_FLOW`; the candidate accepts them.
The three new negative controls keep the **direct**
`ANUBIS_IMPLICIT_FLOW` refusal, as do the pre-existing PC negatives that had
that direct finding on the baseline. The reachable `Row.n` negatives still
lack a writer-located PC diagnostic. All 22 REJECT-intent D9q sources still
refuse. The remaining 40
cases have byte-identical stdout and stderr; nine ACCEPT-intent cases still
refuse and remain precision defects. The [native Safe run](native-positive.stdout)
of the renamed positive exited successfully and printed `7`, the unchanged
outer public value. This is one concrete execution witness, not proof of all
inputs or compiler/runtime correspondence.

The prepatch binary was built from `fa52fbff8d345d4234ccc72e90e21fa2919bd85a`;
the intervening fixture commit changed only matrix sources and the registry.
The candidate binary was built from the exact modified compiler source later
committed at `e4bfe300`; a clean-head incremental rebuild retained its SHA-256.
This is a source-bound local build, not independent clean-room reproduction.
The focused [Rust tests](focused-tests.log), [Clippy](clippy.log), format check,
candidate [build](candidate-build.log), and [clean-head rebuild](clean-head-build.log)
passed under the recorded conditions. The first focused-test command named
the CLI package, which has no library target; the corrected compiler-package
run passed. The first prepatch build attempt failed before Cargo because the
user bus environment was absent; the corrected scoped build passed. Both
instrument failures and their raw hashes are in the machine receipt.
The fixture-commit [matrix harness](matrix-harness.log) passed its own unit
checks, with its [command and exit manifest](matrix-harness.json) retained;
it did not grade the full soundness corpus. The format exit is
[retained](format.exit).
The [documentation-drift report](docs-drift-report.json) passed on the final
documentation draft; its human-readable output is [retained](docs-drift-report.txt).
A prior attempt failed closed while `docs/MISSION.md` was edited during the
derivation. Its [raw derivation error](docs-drift-concurrent-edit.err) is
retained; the stable rerun passed.

The prepatch build and the recorded compiler test/build logs replace the
private home prefix with `<HOME>` in this public receipt; the raw log hashes
remain recorded. The [sanitized independent design review](INDEPENDENT_DESIGN_REVIEW_PUBLIC_SANITIZED.md)
is a redacted copy of one scratch review, not a second review. The earlier
[static review](INDEPENDENT_STATIC_REVIEW.md) gave GO for lead-owned testing
only. The applied-diff review and actual source-matched outcomes are separate
evidence.

This slice does not restore public-field precision across calls or aliases,
fix `For`/`WhileLet` binders, establish writer-located PC diagnostics for
reachable `Row.n` writes, cover nested expressions or early exits generally,
or close D9. The full soundness matrix, workspace suite, hosted CI on this
commit, formal gates, guest-only research lanes, and release gates were not
run. The open defects remain in [CLAIMS.md](../../CLAIMS.md).
