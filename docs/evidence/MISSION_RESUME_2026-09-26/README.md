# Mission continuation and Linux CI preparation — 2026-09-26

This is a bounded continuation receipt, not product or trust-chain completion.
The full mandate remains mapped by `docs/mission/REQUIREMENTS.md`; current
soundness claims remain in `docs/CLAIMS.md` and its supporting defect inventory.

## Reconciled starting point

The remote PR #44 head and fetched object were
`f7e906d1a31533e485aaeeac3ca1c2d1d0877c01`. Work resumed in the isolated
`/home/sicarii/Work/anubis-mission-20260926` checkout on
`codex/mission-round2-20260926`. The ambient `Projects/anubis-lang` checkout is
older and dirty; it was not reset, stashed, built in, or committed from.
`starting-state.json` records full historical revision identities, ancestry
command statuses, worktrees, environment and tool versions. Ancestry observations
are not a review of the contents or proof of the historical receipts.

Both owner-supplied attachments are retained byte-for-byte as `pasted-text-1.txt`
and `pasted-text-2.txt`. The earlier formatted mandate remains unchanged.
The active-lead mandate supersedes historical lead names and routine phase-stop
requests. Lead-only builds, explicit staging, separate code/documentation commits,
and existing guest/isolation requirements remain in force. Do not append another
agent's identity to commits made by this continuation.

The host observation is Arch Linux ARM, aarch64, a QEMU guest. `/dev/kvm` and
`bwrap` are present; Tart is unavailable. No crash, fuzz, research or exploit
runtime witness was obtained or substituted with a capped host process.
The expanded vault backup was observed live as restic PID `1456518` under
scheduler PID `1454155`; `project-vault status` had no completed snapshot.
Neither backup completion nor a full-read/restore drill is claimed.

SIA supplied model-origin handoff pointers, with its live-senses/chains-pass/graph-
complete boundary. Repository files and command output were used to recheck them.
SIA was not reconfigured, restarted or made a build dependency.

## Linux ordinary lane

Code commit `6f038dcb` integrates the already prepared Linux workflow and driver
from `/home/sicarii/.cache/anubis-wt/codex-linux-ci`, preserving that worktree.
Its manifest's source hashes still matched the current integration targets.
Independent static review found no actionable defect in scope; execution,
actual service admission/cancellation and native hosted results were outside
that review's evidence.

Validation performed on the copied files:

- `python3 -B -m unittest discover -s scripts/ci -p test_linux_native.py`:
  **16 tests, OK**. Full output: `linux-preparation/controls.txt`. The printed
  `RuntimeError: missing scope` is a negative-control diagnostic; unittest
  reports success. These are synthetic/mock controls, not native execution.
- `bash -n scripts/ci/linux_scope.sh`: exit 0.
- `classified_tests()` against the current source and committed manifest:
  returned the complete exact roster without error.
- Independent static review covered finalization after teardown, source and
  executable binding, cancellation ownership, exact test scoring and the finite
  test-source classification. No remote review was submitted.

The lane contract is `scripts/ci/linux_native.md`. This is supplemental CI and
does not satisfy the existing full hosted roster, full workspace, Linux bootstrap,
Omarchy installation or release/platform acceptance. Native results on the
configured architectures must be inspected after the workflow runs.

## Compiler continuation

The complete IFC v2 round-2 report and original verifier data are now recovered
under `../IFC2_ROUND2_2026-09-26/`. Parent verification checked the recovered
source hashes and all canonical finding programs with no mismatch. Historical
confirmation is not reclassified as current verification or approved isolation.

The current min/max comparator candidate remains unintegrated. Baseline runtime
reproduction confirmed secret-dependent callback output; independent candidate
review found a new refusal of a valid constant-key callback. The required valid
case remains ACCEPT in the candidate matrix. Do not ship the compiler patch or
report the finding closed until both safety and precision checks pass.
Its next dependency is a reviewed repeatable-key/equality model, not removal of
the valid control. See `../MINMAX_CANDIDATE_2026-09-26/` for the continuation record.

At inspection, PR #44's existing hosted jobs `36219564549` and `36219563067`
reported `in_progress`, with host-verifiable gates running; no restart was
performed. The PR body still described `8548248c`, so it requires a source-current
refresh. Empty review-thread and review lists were read with pagination exhausted.

Before publication, a fresh fetch found remote head `b807e0a7cdd759b094a65810f3a90fa923bad6dc`,
a clippy follow-up changing `ifc2/eval.rs` and `ifc2/value.rs`. The PR body had
also been refreshed by the other session. Independent CI/evidence commits are
to be applied on that fresh head in a clean publication worktree; the failing
compiler candidate remains based on the recorded earlier source. No remote
changes are overwritten. See `prepublication-pr.json` for that observation.
