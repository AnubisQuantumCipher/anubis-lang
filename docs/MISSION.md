# Anubis completion mission — index

This is an index, not a status database. The authorities stay where they are:

| Authority | Location |
|---|---|
| Completion contract and evidence coverage | [Owner mandate](mission/EXECUTION_MANDATE_2026-09-23.md) and [requirement map](mission/REQUIREMENTS.md) |
| Defect registry (open and closed soundness defects) | [`docs/CLAIMS.md`](CLAIMS.md) |
| Execution roadmap | [`docs/COMPLETION_BLUEPRINT.md`](COMPLETION_BLUEPRINT.md) and [`docs/language/ROADMAP.md`](language/ROADMAP.md); reconciliation with [`docs/ROADMAP_AI_ERA.md`](ROADMAP_AI_ERA.md) is in progress (see Review) |
| Soundness acceptance matrix (stable case ids, intended semantics, append-only outcome history) | [`tests/soundness/matrix/`](../tests/soundness/matrix/) — `registry.tsv`, `history.tsv`, `run.sh` |
| Dated receipts | [`docs/evidence/`](evidence/) |
| Subsystem review reports | [`docs/mission/review/`](mission/review/) |

The current `tests/soundness/matrix/run.sh` records a source- and binary-bound
provisional receipt. It always reports `INCOMPLETE` until a reviewed canonical
diagnostic-expectation manifest and safe history publication exist. Older
`history.tsv` rows remain historical measurements, not results of the new runner.

## Finish lines

1. **Anubis 1.0 product completion** — frozen language surface, supported platforms, developer tools,
   assurance profiles, packages, production applications and release process meet the acceptance
   contract.
2. **Full trust-chain completion** — production-linked formal correspondence and full self-hosting for
   the declared semantic surface, with the remaining external trusted computing base stated.

The mission is not finished until both hold. A 1.0 checkpoint may precede the second.
The owner's mandate authorizes continuation across routine phase boundaries once criteria and
receipts hold, and makes the full trust chain mandatory despite older "unscheduled research"
wording. Required valid workloads must be restored; documenting their refusal is not completion.
Mandatory disposable-guest witnesses and merge/release authorization remain unchanged.
See the [authority reconciliation](mission/REQUIREMENTS.md#authority-reconciliation).

## Outcome vocabulary (release contract)

| Outcome | Meaning |
|---|---|
| Verified property | every obligation required for the named property and profile is accounted for and discharged under recorded assumptions |
| Disproved | a checked counterexample to the encoded claim; any model-vs-source gap is labelled |
| Undecided | unresolved, with a typed reason (unsupported encoding, analysis imprecision, budget, solver unknown) |
| Runtime enforced | enforced by a specified runtime mechanism; not a static proof |
| Invalid input / tool error | syntax, typing, infrastructure or internal failure; never a disproof |
| External / not run | a required witness was not obtained; never substituted |

## Starting point (recorded 2026-09-23)

| Fact | Value |
|---|---|
| Working branch | `mission/anubis-1.0`, stacked on `item21/soundness-slices` |
| `origin/main` | `e34d0c89c2181e8bd02ff52b0ab5d50cf97bac4b` (PR #43) |
| Evidence stack (pushed as `origin/evidence/validate-replays-proofs`, no PR) | 12 commits `6aa6fd92`..`7220c4b15e6c6bfd5527e9197b3c7dcb2c008965` |
| Item-21 stack (local, not pushed) | `13373e314d4b7d7e728ec63bc12e967c7acfcff8`, `029d5538151a545f3418b1d6e8932370d60a2e13`, `84296cef66c2d7aadf0c02393bf6d16324a7e29c`, `c87ad1cfa8d279435a094304515e9d9edb8ead5e` |
| Open PRs | #1 (draft, 2026-07-26, unrelated) |
| Host | Omarchy 4.0.3 on Arch Linux ARM, aarch64, **QEMU guest**; `/dev/kvm` present; no Tart; bwrap, Landlock and seccomp available |
| Toolchains | rustc 1.98.1 stable (repo pins nightly-2026-05-10), z3 4.16.0 (CI pins 4.15.4), Lean 4.32.0 |
| Reproducible baseline | `7220c4b1` rebuilt in an isolated worktree reproduces the original item-21 pin byte-for-byte (sha256 `4ddc4ca288b51d95f0a90cbdc49aad881815661539184d5aaeac3256db8cc0ea`) |

"9 of 11" in earlier notes means nine of the eleven leaks in the original reproduction. The expanded
matrix is the reference for open defects.

## Integration units

1. Evidence stack (`6aa6fd92`..`7220c4b1`) — needs its own PR with the template's evidence.
2. Item-21 slices (`13373e31`..`c87ad1cf`) — PR based on (1).
3. Mission work on `mission/anubis-1.0` — split into reviewable PRs as it lands.

## Current checkpoint and next executable step (2026-09-26)

The isolated evidence stack through `8869116a` contains per-obligation solver
replay reporting (`47be70a8`), typed undecided outcomes and PCA v3 counts
(`49671337`), Safe package admission restrictions (`610fe01e`), evidence and
signature checks (`fd206be0`), sealed source-leaf bytes (`4a64f3fa`), and a
provisional matrix runner (`8869116a`). These are local code units, not a
source-bound release or a hosted green result for this exact head. In
particular, source leaves bind the bytes *supplied* to the bundle; they do not
prove which modules and dependencies the resolver selected. The package gate
therefore refuses multi-module and transitive dependencies, even though both
are required valid workloads.

The lead's local full compiler-library run on this code reported **958 passed,
1 failed**. The failure is
`phase6_package_tests::phase6_transitive_path_deps_lock_and_mount`, which now
receives `ANUBIS_DEP_PROOF_UNVERIFIED` because a transitive package manifest is
outside the PCA source closure. This is an unmet required gate, not an accepted
compatibility change. Focused tests and earlier hosted runs do not replace it.
The new matrix runner reports `INCOMPLETE` by construction, so it has not
established a current silent-accept total or accepted-program count. Mandatory
guest, full workspace, platform, final diff review, and clean-source artifact
gates remain separate.

Next, connect the package resolver's actual mounted source/dependency graph to
the analyzed program and evidence claim, restore the valid transitive package
test, and rerun the full compiler suite. Then build a source-bound CLI from the
reviewed code head, validate the retained evidence fixtures, and grade the
registered matrix with reviewed typed expectations. Continue the distinct
[match/if-let contract mechanisms](evidence/ARM_BINDER_REGISTRATION_2026-09-26/README.md)
and [min/max precision controls](evidence/MINMAX_REPEATABILITY_2026-09-26/README.md)
without treating a refusal of valid code as completion. The full dependency map
remains [here](mission/REQUIREMENTS.md); draft PR #44 still requires a reviewable
split and human review before protected-branch integration.
