# Continuation checkpoint — 2026-09-26

This goal turn made implementation and evidence progress. Neither completion
finish line is achieved; the mission remains active.

Publication worktree: `/home/sicarii/.cache/anubis-wt/codex-proof-bool`, branch
`codex/proof-bool-20260926`. Code commit:
`b835b94b44fa8705dd973eb1133f862706fd04a8`, based on remote `f48a5c9a`.
The bool fix, tests, matrix cases and classified native test target are committed
separately from this evidence. PR44 remains the draft integration stack, not a
mergeable whole-stack release. Re-query the remote before further publication.

The full CLI build from that worktree **completed with exit status 0**; session
**77431** is no longer pending. The command was
`cargo build --locked --release -j 2 -p anubis`, using the existing capped runner,
explicit user runtime/D-Bus environment and
`CARGO_TARGET_DIR=/home/sicarii/.cache/anubis-wt/target-codex-round2`.
Its copied log and exit receipt are `cli-build.txt` and `cli-build.rc` beside this
checkpoint. The build began at `b835b94b` and ended after documentation commit
`e3b8e1a11cd1dc05c76419bce56009fd901a819d`; compiler/solver/CLI/vendor/toolchain
inputs remained unchanged as recorded in `source-stability.json`. Do not rewrite
the original source manifest's code-commit binding.

The immutable technical CLI pin is
`/home/sicarii/.cache/anubis-item21/pins/anubis-proof-bool-c91445cdb04faab8`,
SHA-256 `c91445cdb04faab827925289a1c426ac358d4491ae6732f63d1bfb752a4713e7`.
`cli-pin.json` records its attribution. The registered finite bool controls now
report **14 PASS** observations across full Safe checking and IFC2-only reports,
with harness exit status `0` and an unchanged binary hash. Complete commands,
sources and stdout/stderr are retained in `cli-checks/results.json`; the focused
full-Safe history rows are appended once under `proof-bool-focused-b835b94b`.
The build/check subtask is finished; broad workspace, matrix, guest-journal and
platform gates remain open. This pin is not a release or independent rebuild.

Min/max worktree: `/home/sicarii/.cache/anubis-wt/codex-minmax-repeatability`,
branch `codex/minmax-repeatability-20260926`, based on `f48a5c9a`, still dirty
with its **withheld** candidate. The complete patch and evidence are in the
adjacent MINMAX_REPEATABILITY directory. Its CLI build was deliberately canceled
with exit 143 after confirmed compatibility regressions; session 67290 is finished.
The expanded test executable is retained immutably as recorded in
`candidate-test-pin.json`. No min/max code is in the bool commit.

The active min/max extension worktree is
`/home/sicarii/.cache/anubis-wt/codex-minmax-extended`, based on `e3b8e1a1`;
its work follows the extension design plus independent design review. This bool
receipt establishes no result for that separate candidate. Stable unary/comparison
terms and resolved helper frames need runtime parity, shared budgets and
full-Safe/IFC2 entry-point precision tests.
Do not treat typed/inferred helper annotations or scalar abstract equality as
runtime identity evidence. Preserve all required passing tests. All reviewers
so far were static; lead execution is separately recorded in logs.

Preserve the ambient dirty `Projects/anubis-lang` checkout and previous worktrees,
including `/home/sicarii/Work/anubis-mission-20260926`; neither is the publication
checkout. The active goal still includes full review, remaining round-2 fixes,
the production trust chain, full self-hosting, applications and platform/release
acceptance. Required crash/fuzz/research/exploit witnesses still need the mandated
disposable guest; there is no local Tart substitution.

## External jobs and backup

Re-poll actual hosted handles rather than restarting for elapsed time. Last
observed `f48a5c9a` Linux runs: **36220631525**, **36220630178**, in progress.
Existing hosted runs **36220630092**, **36220631584** were running/queued.
New publication will have new run handles; record their observed state separately.

The expanded project backup is **partial**, not live or complete. Restic saved
snapshot `e1957358934fa1cf85d33e108a94ad529574454c3a69d1c6fd56a6d83da59d59`
then exited 3 on unreadable WORLDLINE overlay work directories; its scheduler
failed with exit 1. The catalog still had `latest_snapshot: null`, and no initial
full-read/verified-restore receipt existed. The old backup PIDs are gone.
The existing timer remained enabled; re-query before assuming another job's state.

Read-only diagnosis and exact paths are retained locally at
`/home/sicarii/.cache/anubis-item21/backup-failure-20260926/REPORT.md`.
No exclusions, permissions, source files, service settings or repository packs
were changed. Completing coverage needs a reviewed permission-preserving source
reader/export path, followed by the serialized backup, full read and restore drill.
Verifying the partial snapshot alone cannot repair its omissions.
