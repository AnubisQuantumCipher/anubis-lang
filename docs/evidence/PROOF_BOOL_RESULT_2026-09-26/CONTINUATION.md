# Continuation checkpoint — 2026-09-26

This goal turn made implementation and evidence progress. Neither completion
finish line is achieved; the mission remains active.

Publication worktree: `/home/sicarii/.cache/anubis-wt/codex-proof-bool`, branch
`codex/proof-bool-20260926`. Code commit:
`b835b94b44fa8705dd973eb1133f862706fd04a8`, based on remote `f48a5c9a`.
The bool fix, tests, matrix cases and classified native test target are committed
separately from this evidence. PR44 remains the draft integration stack, not a
mergeable whole-stack release. Re-query the remote before further publication.

The full CLI build from that worktree is running in exec session **77431**:
`cargo build --locked --release -j 2 -p anubis`, using the existing capped runner,
explicit user runtime/D-Bus environment and
`CARGO_TARGET_DIR=/home/sicarii/.cache/anubis-wt/target-codex-round2`.
Actual log and exit receipt: `out/proof-bool/cli-build.txt` and
`out/proof-bool/cli-build.rc` (the latter exists only after completion).
Do not start a duplicate build. On success, copy and hash a new immutable CLI
pin, bind it to the recorded source manifest, run the registered finite bool
checks and controls, and retain actual matrix history. Do not call the current
library-test executable a CLI pin.

Min/max worktree: `/home/sicarii/.cache/anubis-wt/codex-minmax-repeatability`,
branch `codex/minmax-repeatability-20260926`, based on `f48a5c9a`, still dirty
with its **withheld** candidate. The complete patch and evidence are in the
adjacent MINMAX_REPEATABILITY directory. Its CLI build was deliberately canceled
with exit 143 after confirmed compatibility regressions; session 67290 is finished.
The expanded test executable is retained immutably as recorded in
`candidate-test-pin.json`. No min/max code is in the bool commit.

The next min/max implementation must use the extension design plus independent
design review. Stable unary/comparison terms and resolved helper frames need
runtime parity, shared budgets and full-Safe/IFC2 entry-point precision tests.
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
