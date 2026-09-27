# Captured solver process outcomes — scoped local receipt

Code commit `2683669c2ae87ceea66d19ebb0e6995578c289eb` distinguishes a missing
Z3 executable from a present process that fails to spawn, accept the query,
complete its write/wait cycle, or return an admissible response. A failed
present tool is a typed tool failure in the captured producer and native
cross-check paths, not permission to accept a native verdict without the
configured comparison. The captured inventory consumer preserves that typed
failure. The public `SolverCheck` wire schema is unchanged.

The response validator requires the observed model-unavailable response,
expected exit and empty stderr for a primary `unsat` or `unknown` query that
also asks for a model. A `sat` answer must have one bounded model-list
envelope. Malformed, extra, rejected, wrong-exit and stderr-bearing responses
are refused before they can become proof-bearing answers. The envelope is a
framing check: it does not prove that model terms have the right sorts, cover
all declarations, replay array values, or correspond to executable source.
The legacy optional raw-query helper is not promoted into this typed captured
producer contract.

The [machine-readable gate receipt](gate-receipt.json) binds the code commit,
source files, `Cargo.lock`, the reviewed patch, toolchain, tested CLI, complete
commands, exit codes, raw-log hashes and retained sanitized logs. The lead
ran focused fake-process controls, captured solver/project/module tests,
direct-PC tests, the compiler solver tests, certificate-coverage tests,
workspace format, strict compiler Clippy, and a locked CLI build. All recorded
gates exited successfully. The fake-solver scripts ran through stable
`/bin/sh`, exercising the process pipe/write/wait path; a direct
broken-executable spawn control remains. Earlier failing drafts remain
failed evidence and are not counted as passing retries. The cause of the
transient script `ETXTBSY` failures is an inference, not a proved OS
diagnosis.

A clean build at the committed HEAD exited successfully and produced the
same SHA-256 CLI as the immutable tested snapshot:
`d5b452c21c2a92d39199a8e93b2a5e9d1fc642fbbda493f45ef88a871448bcfd`.
An initial clean-build wrapper stopped on a copied expected-digest
precondition before Cargo ran; that failed wrapper and output remain in the
local raw record. The corrected wrapper generated the clean-build manifest
and log identified in the receipt.

Selected source-matched Safe CLI comparisons to the preceding D9 pin found
identical raw output on all recorded D9q and Family-1 cases. The compact
[comparison summary](selected-check-summary.json) retains case identities,
source hashes, intent, exits and typed diagnostic summaries; the raw JSON
hashes are in the gate receipt. These selected comparisons do not establish
whole-corpus stability. In particular, nine D9q `ACCEPT` cases still refuse;
the direct public-field and public-sibling cases remain examples of a
precision defect, not successful higher-order or field-qualifier support.

The [independent applied-diff review](INDEPENDENT_REVIEW.md) gave a scoped GO
for this source-bound code unit after inspecting the actual diff, source
hashes, logs and selected comparison records. The reviewer did not build or
run the compiler. Its printed `captured.rs` digest contains an extra digit;
the original report is preserved with a [transparent erratum](REVIEW_ERRATUM.md)
and a corrected machine receipt. Its historical `../lead-v11/` references describe the
retained local raw-evidence layout; the corresponding public logs and compact
comparison are linked here. Published log normalization replaces the checkout, isolated Cargo target
and local evidence path prefixes with `<checkout>`, `<cargo-target>` and
`<local-evidence>`, and trims extra terminal blank lines to one final newline;
the receipt records both raw and retained SHA-256 values. Raw logs and full selected-output JSON remain
preserved locally. No private machine path or raw model text is published here.
The [independent staged-documentation review](INDEPENDENT_STAGED_DOCS_REVIEW.md)
checks the exact preceding receipt diff and its retained evidence; this link
was added afterward without changing the reviewed machine receipt or logs.

This unit does not close `A-EVID-1` counterexample replay: array values are
not pinned, string-only models are not replayed as values, and omitted scalar
bindings may remain free. Source-to-obligation completeness, SMT encoding
correspondence, all valid Z3 response shapes, and child cleanup after a
failed `wait_with_output` also remain unproved. The full workspace suite,
full matrix, hosted CI, formal correspondence, disposable-guest seal and
independent clean-room reproduction were not run for this code commit. No
release or public captured-build admission claim follows from this receipt.

The next dependency is a declaration-by-declaration, typed model inventory
with executable replay controls and honest `undecided` outcomes for missing
bindings. In parallel, the remaining D9 field/call qualifier and valid-case
precision work needs independent design review and same-source negative and
positive comparisons before integration.
