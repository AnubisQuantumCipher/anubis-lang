# Match and if-let contract registration

Fixture and registry commit:
`e4f9bcb70530ace15033fd5e3db17e77c1d9753f`. The focused checker
results and each fixture's SHA-256 are in [results.tsv](results.tsv); the
binary identity and source-input comparison are in [manifest.json](manifest.json).
Required valid call-position controls were frozen in
`801766852e6c3e730ba5b4f0a3f2bc75efff2d5d`, before a compiler repair.
Their source hashes and checker outcomes are in [controls.tsv](controls.tsv).
The pinned baseline accepted the satisfied scrutinee, guard, and if-let calls,
the short-circuited false guard, and the later guard after an unguarded wildcard
arm. All are `ACCEPT` requirements for the repair, not permission to turn a
violated call into a refusal by rejecting these valid forms too.

The later control freeze at `5e4604d2ca9204787ac4a6d58ec98b0d1ea14094`
adds valid `let x = 1` paths through pure calls in a match scrutinee, a match
guard, and an `if let` scrutinee. The reached body calls `g(x)` with
`requires(x > 0)`; a repair must retain the independent outer fact. The same
immutable checker pin accepted each source with exit code 0 and a JSON `pass`
summary. Exact source hashes and outcomes are in
[outer-fact-controls.tsv](outer-fact-controls.tsv). The checker first failed
to start because the shell lacked the user-bus environment; those
infrastructure failures are retained only in ignored local logs and were
re-run with the established user-bus variables. They were not classified as
program results. The compiler-input diff from the pin's code commit to the
control freeze was empty (`git diff --quiet` exit 0). These passing summaries
do not by themselves establish which named contract obligations were emitted;
obligation-level regression tests are required for the candidate.

Independent review then found two more reachability and state controls. A
literal first arm makes a later violated guard unreachable; an untaken first
arm's assignment cannot invalidate an outer fact needed by a later guard.
Their source freezes are `d7c110c174e141dba12f72e6a23de0ac78dbfcb2`
and `7162d2fce8ebba276132cb0b46aa7543fbcd17aa`; the baseline pin accepted
both with exit code 0 and JSON `pass`. Source hashes and outcomes are in
[literal-reachability-control.tsv](literal-reachability-control.tsv) and
[untaken-write-control.tsv](untaken-write-control.tsv). Compiler inputs still
match the pin's recorded code commit (`git diff --quiet` exit 0 through the
latter freeze). A candidate that rejects these valid paths has a precision
regression even if it rejects the violated reachable guard.

The `594c51621b96a55d8ba6d981749017f3d569ffe8` registration adds a
guard assignment-RHS call and its valid twin. The same source-bound pin
silently accepted `b = f(-1)` in an evaluated match guard, while its direct
call twin was **disproved**; it also accepted `b = f(1)`. The result and source
hash for each form are in [guard-assignment-results.tsv](guard-assignment-results.tsv).
The source-input diff from the pin's code commit to this registration was
empty (`git diff --quiet` exit 0). This is another missing call position, not
evidence that a repair of the simpler direct guard expression is complete.
Independent review found that the scrutinee and wildcard-guard cases have no
sibling binder, so their family was corrected from the initially registered
`M-ARM-BINDER` to `M-CALL-POSITION` in
`6a816dad7d9c07bd19310169d39be0fd7ef1460d`. Stable case IDs, sources,
intents, categories, and observed outcomes did not change.
The binary is the previously recorded immutable proof-bool CLI pin; its full
SHA-256 is in the manifest. The compiler, CLI, solver, lockfile, and vendor
input diff from that pin's recorded code commit to the fixture commit was
empty (`git diff --quiet` exit 0). This links the focused check to unchanged
compiler inputs; it is not an independent rebuild claim.

The checker silently accepted violated `requires` calls in a `match`
scrutinee, a `match` guard, and an `if let` scrutinee. Each direct-call twin was
disproved. The same-binder violated cases were **undecided**, which is an
explicit refusal but not a checked disproof. Valid same-binder, dead-path, and
guard-established controls were also **undecided**, so the existing analysis
has precision defects as well as missing obligations. The quoted-string
control checked clean. The result classes and nonzero exit codes are recorded
per case in the TSV; no parser or type error was credited as a rejection.

These cases were appended to the canonical matrix registry and their focused
observations to its append-only history. Each history append contains only the
new rows, not another copy of the whole matrix. The checker ran on ordinary
Safe source only. Neither program execution nor the full matrix, crash,
research, fuzz, or disposable-guest lanes ran for this receipt. Runtime
witnesses, final fix verdicts, and broader precision regressions remain open.
The documentation-drift gate passed on the registration and correction tree:
37 stamps checked, zero drift. That gate does not establish the matrix verdicts;
the focused results above do.

The arm-binder implementation draft is excluded from this commit. Independent
review found that a blanket unresolved binder result rejects valid calls and
dead paths, while string-based symbol matching can mistake quoted text for a
binder. A sound next change must inspect scrutinee and guard calls in their
actual evaluation order, model each lexical binding separately, and prove
valid guarded and dead controls as well as rejecting reachable violations.
