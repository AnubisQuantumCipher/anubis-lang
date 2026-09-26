# Captured Safe solver-stream integrity — scoped local receipt

Code commit `ee4f13bb499059f51055f5a3b451abe15c5920ca` gives the private
captured Safe compiler path a typed `SolverStreamInvalid` failure for empty or
malformed solver-check streams. It rejects the legacy synthetic integrity
marker by its reserved name or exact detail, invalid wire statuses, and a
malformed no-obligations sentinel before interpreting obligation rows. Valid
wire `FAIL` and `UNKNOWN` rows remain distinct raw outcomes in
`SolverRefused`; this change does not declare a `FAIL` to be a checked
counterexample. The legacy flat refusal helper remains in use elsewhere.

The [machine-readable gate receipt](gate-receipt.json) binds the code commit,
final source hashes, exact focused commands, their exit codes, raw-log hashes,
and retained logs. The retained logs replace the local home-directory prefix
with `<HOME>/` and trim terminal blank lines; their message lines and test
results are otherwise unchanged. The focused stream-integrity test passed,
the captured resolver suite reported `26 passed; 0 failed`, compiler Clippy
with `-D warnings` passed, and workspace format, diff, and documentation-drift
checks passed. Cargo
ran in the lead-owned memory-capped user scope with an isolated target. The
[independent final-diff review](INDEPENDENT_REVIEW.md) gave a bounded GO
against the final source hashes; the reviewer did not run tests.

This validates stream **shape**, not completeness. The checker does not yet
compare returned rows against `tainted.solver_obligations`; an omitted row or
substituted exact no-obligations marker could pass this validator. Ordinary
solver/tool errors are still encoded as wire `FAIL` by existing producers.
The public CLI does not consume this private typed result, and no native
artifact, evidence seal, offline proof authentication, full workspace or
matrix result, hosted witness, or disposable-guest seal follows from this
receipt. The next dependency is a source-to-row inventory check and typed
tool-error classification before public admission can use these outcomes.
