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
The fixed-path canonical inventory loader is banked at `b3ddf665` with a
[scoped receipt](evidence/MATRIX_CANONICAL_LOADER_2026-09-26.md); no live
canonical manifest is checked in and no full-matrix completion is claimed.

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
| Evidence stack (pushed as `origin/evidence/validate-replays-proofs`, no PR) | 12 commits `6aa6fd92`..`7220c4b15e6c6bfd5527e9197b3c7dcb2c008965` (historical entry; corrected below) |
| Item-21 stack (local, not pushed) | `13373e314d4b7d7e728ec63bc12e967c7acfcff8`, `029d5538151a545f3418b1d6e8932370d60a2e13`, `84296cef66c2d7aadf0c02393bf6d16324a7e29c`, `c87ad1cfa8d279435a094304515e9d9edb8ead5e` |
| Open PRs | #1 (draft, 2026-07-26, unrelated) |
| Host | Omarchy 4.0.3 on Arch Linux ARM, aarch64, **QEMU guest**; `/dev/kvm` present; no Tart; bwrap, Landlock and seccomp available |
| Toolchains | rustc 1.98.1 stable (repo pins nightly-2026-05-10), z3 4.16.0 (CI pins 4.15.4), Lean 4.32.0 |
| Reproducible baseline | `7220c4b1` rebuilt in an isolated worktree reproduces the original item-21 pin byte-for-byte (sha256 `4ddc4ca288b51d95f0a90cbdc49aad881815661539184d5aaeac3256db8cc0ea`) |

"9 of 11" in earlier notes means nine of the eleven leaks in the original reproduction. The expanded
matrix is the reference for open defects.

Correction 2026-09-26: Git's inclusive `e34d0c89..7220c4b1` range contains
13 commits. The first [local PR 44 split](mission/PR44_LOCAL_SPLIT_2026-09-26.md)
records dependency-correct review refs without rewriting published history.

## Integration units

1. Evidence stack (`6aa6fd92`..`7220c4b1`) — needs its own PR with the template's evidence.
2. Item-21 slices (`13373e31`..`c87ad1cf`) — PR based on (1).
3. Mission work on `mission/anubis-1.0` — split into reviewable PRs as it lands.

## Current checkpoint and next executable step (2026-09-26)

The isolated local integration branch `codex/register-round2-20260926` has
reviewed code through `6b85cf2207e5b3126a18e2df1190a4efa49a2ecc`.
The [guard-transfer receipt](evidence/GUARD_WRITE_TRANSFER_2026-09-26/README.md)
binds the preceding `88a2903c` code, selected Safe checks, a clean-head
binary and a complete provisional same-source matrix comparison. Its reached
shadowed-binder call is now DISPROVED and the required valid failed-guard
fallthrough is accepted. Four valid performance forms timed out in the full
candidate run; their cause is unresolved. One OOM-designated form requires
the mandatory disposable guest for follow-up.
The historical full matrix remains `INCOMPLETE`, with other silent accepts,
valid refusals and unbound diagnostic classes.

The [min/max receipt](evidence/IFC2_MINMAX_CALLBACK_2026-09-26/README.md)
binds `6b85cf22`, its clean-head binary, selected Safe verdicts and finite
native witness. The registered min/max leak pair changed from check PASS to
typed IFC2 refusal. Fixed captured-key comparison, negation, helper and
padded-argument controls still pass, including normal Safe native execution
for the padded min/max examples. Two registered valid unannotated `Row` helper
controls remain refused by D9's public-formal inference; this is an open
precision defect. The min/max result is selected-source evidence, not an
updated full-matrix total or a verified-property seal.

Fixture-only commit `d49f32b1` and its
[source-bound D9 registration](evidence/D9_QUALIFIER_PRECISION_REGISTRATION_2026-09-26/README.md)
freeze public-field projection, alias, forwarding, whole-value, wrapper,
secret-write and computed egress controls. The immutable `6b85cf22` binary
refuses five ACCEPT-intent controls; the clean-Row valid control passes and
all six REJECT controls refuse with typed security diagnostics. Independent
design review approved the oracles and held a call-boundary-only workaround:
the direct source walker and interprocedural summary also collapse protected
fields into root secrecy.

The [PCA v4 check-evidence receipt](evidence/HONEST_FAIL_SOURCE_CHECK_2026-09-26.md)
binds code commit `fc72526b`, focused Safe tests, a local clean-head release
binary and a replayed FAIL/PASS twin. A source-derived security refusal is now
verifiable as a recorded FAIL without claiming that a solver ran or that the
program is approved. Independent static review gave GO for lead testing. The
extra no-default-features Clippy attempt failed; multi-leaf correspondence,
per-obligation backend provenance and archived v3 producer compatibility
remain open. No full workspace gate was run.

The next soundness implementation dependency is a typed distinction between protected
struct fields and whole-value secrecy, carried through direct reads, calls,
aliases, returns, joins and writes. Preserve the negative controls and compare
all frozen forms on the same source-bound pins before claiming restored
precision. In parallel, connect the
[captured source graph](evidence/CAPTURED_COMBINE_ADAPTER_2026-09-26.md)
to checker/lowering and package admission; the valid transitive package still
fails at `ANUBIS_DEP_PROOF_UNVERIFIED`. The D9 scratch v2 patch remains
withheld after independent review found possible label laundering through
uncertified call-returned structs and stale field shapes after nested writes.
Four matched [factory and nested-write witnesses](evidence/D9_QUALIFIER_V3_REGISTRATION_2026-09-26/README.md)
are now frozen and source-bound at `fc72526b`: both REJECT controls receive
typed security refusals, the factory public sibling passes, and the valid
clean nested write is over-refused. The next executable step is independent
review and lead-only testing of a revised D9 scratch patch against all frozen
controls and the unchanged original corpus. The matrix
[canonical-manifest dependency](mission/REQUIREMENTS.md) remains open despite
the new fixed-path loader: production diagnostics still lack complete stable
source locators. Preserve the old `history.tsv` and add compact event history
only after that gate is reviewed. `docs/CLAIMS.md` is the defect authority.

No current full workspace, hosted, disposable-guest, independent clean-room,
protected-branch, product-release, or full trust-chain result is claimed.
The separate main-based front-page correction and the
[PR #44 review stack](mission/PR44_LOCAL_SPLIT_2026-09-26.md) remain local;
protected integration needs human review
and its applicable authorization.
