# Withheld match call-position candidate

This is a **failed, local candidate**, not a closure receipt or release pin. The
compiler change at `cb7fae981706342a3a531dfecdbfed33dbd48d60` makes the
original match-scrutinee, match-guard, if-let-scrutinee, and guard-assignment
`f(-1)` cases produce named `requires@f` disproofs. Its focused results are in
[focused-results.tsv](focused-results.tsv). The M-ARM-BINDER sibling-binder
false accepts and same-binder undecided/precision cases remain visible there.

Independent review found additional regressions. On the exact tracked source
bytes in [review-results.tsv](review-results.tsv), the candidate disproves
`requires@f` after a guard block has returned from `main`, and also through a
write to a callable in a literal match arm whose pattern cannot match. Both
programs are required valid controls. The old baseline accepts them; the
baseline native witness sources in
[`tests/soundness/matrix/witnesses.tsv`](../../../tests/soundness/matrix/witnesses.tsv)
print `START` and `SAFE`, respectively, without reaching `BAD`. These are
precision failures, not soundness fixes.

The nested guard `Stmt::If` case remains a false accept on both pins. Its
nonmodelable `pred()` condition takes a body that calls `f(-1)`, but the checker
emits no `requires@f` diagnostic. Its direct `f(-1)` twin is disproved on both
pins; the satisfied and untaken controls are accepted. The guard-assignment
dead-branch and path-guarded controls are also accepted on both pins. The
tracked case files match the probed source files byte for byte; source hashes
and the local-log hashes are in the review table. Raw logs remain ignored
because they contain machine-local paths. The table does not infer exit codes
that the review probes did not retain.

The technical candidate pin is `anubis-mcall-b3099e9e`, SHA-256
`b3099e9e768f1faff71a5ae830e933f9e349e463546ca179b5f4c6abd5ad496c`.
The baseline pin SHA-256 is
`c91445cdb04faab827925289a1c426ac358d4491ae6732f63d1bfb752a4713e7`;
its compiler source commit is
`b835b94b44fa8705dd973eb1133f862706fd04a8`. The candidate was built at a
clean `cb7fae98` tree under the lead's build lane. The release-build log ends
with `Finished release`; the all-targets Clippy run with `-D warnings` ends
with `Finished dev`; the focused library tests named in
[manifest.json](manifest.json) report `ok`. The lead reported `cargo fmt --all
-- --check` passing, but retained no separate format log. None of this is a
whole-workspace, guest, hosted, or independently reproduced witness.

`git diff --quiet cb7fae98..0f34f186 -- .cargo compiler solver tools/anubis
Cargo.toml Cargo.lock rust-toolchain.toml vendor build.rs` returned exit code
`0`; the review fixtures added after the build did not change those compiler
inputs through that fixture commit. Later local compiler edits are outside this
receipt. The earlier passing-obligation test at `cb7fae98` used inline source;
the subsequent exact tracked-fixture obligation test was added separately and
is not claimed as part of the `cb7fae98` build.

The append-only matrix history records the focused candidate observations and
the baseline/candidate review probes under distinct labels. A repaired compiler
needs a new source-bound pin, checker results for these exact cases, the
required positive obligation controls, and review of its final diff. This
candidate must not be pushed as a soundness closure.

## Subsequent provisional repair

The lead then built a separate, local compiler at
`8f1589410c85a0f5b982913cb6a8bf089e89f464` (Git tree
`578308880ee3fbca745557be46d96cfb7a11fb9c`), technical pin SHA-256
`8aeab6a60607782c766e5a75aa0e535448351de8e9ddb68de8c9f1847ba4f3ff`.
Its [focused results](repair-focused-results.tsv) use tracked fixture bytes
matching every recorded source hash. The early-return and untaken-alias valid
programs return to `ACCEPT`; the exact tracked-fixture positive-obligation
test `round2_source_bound_match_controls_emit_passing_requires` reports `ok`.
This is progress, but the nonmodelable-if violated case and its unreachable
control both return `UNDECIDED`, which is the wrong class. M-ARM-BINDER still
includes silent accepts and unresolved valid controls. The provisional repair
has no closure claim, final-diff independent review, full matrix, or release
witness. Its focused results are intentionally not appended to matrix history
until the next reviewed candidate is settled.

After adding this receipt and the append-only history rows, the lead ran
`scripts/run_docs_drift_gate.sh`; it reported `DOCS_DRIFT_GATE: PASS`, with 37
stamps checked and zero drift. The ignored log hash is in the manifest. This
gate checks documentation stamps, not the semantic correctness of the withheld
compiler change.
