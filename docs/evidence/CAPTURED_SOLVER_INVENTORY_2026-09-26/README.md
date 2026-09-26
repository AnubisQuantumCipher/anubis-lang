# Captured Safe solver-row inventory — scoped local receipt

Code commit `aa9c33b76a9c69607a53627e61e01e0db5abcab6` attaches an
in-process source ordinal to each solver row while the producer visits the
final `TypedIR.solver_obligations` inventory. The private captured Safe bridge
then compares the returned stream with that inventory: exact empty-inventory
sentinel, row count, ordered origin, and display name. Missing, extra, copied,
reordered, or renamed issued rows become a typed compiler integrity failure
before captured lowering. The ordinary public `SolverCheck` wire shape and
legacy caller result remain unchanged. Real wire `FAIL` and `UNKNOWN` still
reach the separate refusal path; neither is silently accepted.

The [gate receipt](gate-receipt.json) binds the code commit, source hashes,
commands, exit codes, raw-log hashes and retained logs. The focused captured
resolver suite passed with `29 passed; 0 failed`, including a satisfied
`requires` call and a violated call that reached a replayed solver refusal.
The certificate-coverage group passed with `21 passed; 0 failed`; compiler
Clippy with `-D warnings`, format, diff and documentation-drift checks passed. Cargo ran in the
lead-owned memory-capped user scope with an isolated target. The first Clippy
run **failed** on test-only cloned singleton slices; its complete
[failed log](initial-clippy-fail.log) is retained. After replacing those
slices with borrowed singletons, the final tests and Clippy were rerun.
Retained logs replace the private home-directory prefix with `<HOME>/` and
trim terminal blank lines. The [independent final-diff review](INDEPENDENT_REVIEW.md)
gave a bounded GO against the final file hashes; the reviewer did not run
the gates.

This is row assembly integrity **after** the final typed obligation vector.
It cannot detect an obligation omitted before that vector, prove that a row's
SMT encodes its source assertion, authenticate a model or certificate, or bind
the result to a native artifact. Existing producer tool errors can still
arrive as wire `FAIL`; a downstream consumer must not call a generic `FAIL`
disproved. The public CLI and evidence path do not yet consume these ordinals.
No full workspace, matrix, hosted, package, evidence-seal or guest result is
claimed for this commit. The next dependency is typed producer outcomes and
source-to-obligation correspondence before public admission.
