# Focused match call-site candidate

This receipt covers only the registered `M-CALL-POSITION` and `M-ARM-BINDER`
Safe-mode cases. It is **not** a full soundness-matrix, hosted-gate, release-seal,
or product-completion result. The compiler code commit is
`032bf0b0c6e2ccf13ccf2794ffef4896c3e437e0` (tree
`19073d84a0bec80e5db71b177b1a9d9005db0ca3`). The lead built its clean
worktree with the pinned nightly Rust toolchain on a Linux aarch64 QEMU guest
and copied the resulting binary to a local immutable technical pin, SHA-256
`1410837b5c195d2aaec41f880bd93afc2356c3005e3db2efaa4d5664bf63f026`.
The [manifest](manifest.json) records the Cargo lockfile, toolchain, build-log,
test-log, runner, and result hashes. This is a source-linked local build, not
a byte-for-byte independent reproduction or a sealed release artifact.

The [final result table](final-results.tsv) binds each source hash to the same
baseline, provisional repair, and candidate outcomes. The
[verdict-flip table](verdict-flips.tsv) isolates changes from the provisional
repair. [Checker output](checker-outputs.redacted.txt) retains every focused
run's text, replacing only the local repository prefix with `<REPO>/`; each
row also carries the SHA-256 of its original ignored log. The redacted
[build](release-build.redacted.log),
[compiler-library](compiler-lib.redacted.log), and
[Clippy](clippy-all.redacted.log) logs preserve their reported outcomes without
publishing a local home path. The focused runner was local and ignored rather
than a versioned portable harness; its hash is recorded. The public output
archive and result table are durable, but this is not independent clean-room
reproduction.

All registered `M-CALL-POSITION` carrier cases in this focused run now match
their intended class. In particular, the violated nested call under a reached
predicate is `DISPROVED`, its dead-branch twin is `ACCEPT`, the formerly false
disproof under `pred() && x > 0` is `ACCEPT` when `pred()` is false, and both
direct calls with a same-named conditional obligation retain a replayed
`DISPROVED` result. The valid untaken expression-match callable-write control
is `ACCEPT`. The independent agent review of the final code diff found no new
soundness blocker; that review was static, not an external expert audit.

`M-ARM-BINDER` remains open in the same run: sibling-binder carriers still
silently `ACCEPT`, while several same-binder valid and invalid controls are
`UNDECIDED`. A valid-program refusal is not a completed contract analysis.
The provisional and final same-source comparison is recorded under distinct
labels in append-only `tests/soundness/matrix/history.tsv`; the corrected
arm-tail source is explicitly identified by its content commit and hash.

The source-bound unit controls passed before the code-only commit with the
same tracked code bytes. The full compiler-library run, format check, and
workspace Clippy run passed on that pre-commit worktree; their logs do not
contain a HEAD/clean-tree marker, so they are not presented as exact-commit
hosted witnesses. The release build and focused checker run used the clean
committed code and the pinned binary respectively. Full-matrix, mandatory
disposable-guest, hosted, final workspace, external-review, and release gates
were not run for this candidate. The pre-existing `A-EVID-1` replay sidecar
misstatement remains open: this receipt does not claim that the current
`solver_replay.json` attests to every obligation.
