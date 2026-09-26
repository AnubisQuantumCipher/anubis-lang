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

The latest reviewed implementation code commit on the isolated local integration
branch is `e7187a314db5385e2d34fd74dd3b773878112195`. Fixture-only commit
`60b8d4e429d2f96f54a001e851077c51b1f1c912` follows it; this docs-only
checkpoint records its pinned baseline.
It includes the evidence, package-refusal and provisional-matrix units listed
in [CLAIMS](CLAIMS.md), a source-snapshot check before evidence-producing native
builds, and typed diagnostic refusal context. Commit `72e1390b` adds only the
caller-supplied [source-graph identity primitive](mission/VERIFIED_SOURCE_GRAPH_DESIGN_2026-09-26.md).
The new [bounded byte reader](evidence/SOURCE_GRAPH_READER_2026-09-26.md)
captures a private tree on Linux under an explicit trusted-procfs assumption.
The [private captured-project resolver](evidence/CAPTURED_PROJECT_RESOLVER_2026-09-26.md)
parses those bytes and refuses nested imports explicitly, but the public
checker, lowering, package admission and verifier are not yet bound to that
captured graph. The valid transitive package fixture therefore still fails at
`ANUBIS_DEP_PROOF_UNVERIFIED`; this is an open precision/product gate.
The [request-local solver-symbol receipt](evidence/REQUEST_LOCAL_SOLVER_SYMBOLS_2026-09-26.md)
records same-process and cold-process byte stability in the covered contract
and match obligations. Its source-bound selected Safe comparison shows no
typed verdict flips against the preceding code pin; old evidence sidecars
must still be rederived for the new producer.

The [scoped arm-binder receipt](evidence/ARM_BINDER_SCOPED_FIX_2026-09-26.md)
records the preceding `70b200c7` result. The
[statement enum-payload receipt](evidence/ARM_ENUM_PAYLOAD_PRECISION_2026-09-26.md)
and [value-position enum receipt](evidence/ARM_ENUM_EXPR_PRECISION_2026-09-26.md)
show the frozen valid dead arms accepted and reachable violated twins
disproved in their respective positions. The latter selected comparison
has only the expected classified flips on the same registered Safe sources.
The separately [registered guard-write pair](evidence/MATCH_GUARD_WRITE_REGISTRATION_2026-09-26.md)
is still undecided in both valid and invalid fallthrough forms, and its
direct violated twin is disproved. The valid refusal is a precision gate;
no silent accept was reproduced for that pair.
[Additional frozen controls](evidence/MATCH_GUARD_WRITE_ADDITIONAL_CONTROLS_2026-09-26.md)
cover stale facts after a write, binder shadowing, and a contracted call in
the guard. They preserve the pre-repair typed baseline and runtime paths.
The full matrix runner remains `INCOMPLETE` by design. The local unfiltered
compiler-library attempt on the preceding head was interrupted after a
Research execution test was identified in this Linux guest; it is not a
required suite witness. No disposable Tart, hosted, full workspace,
independent rebuild or release gate is claimed for this head.

Next, restore the required valid guard-write fallthrough while retaining the
invalid control and correct pre/post-write facts. Independently connect
captured bytes to checker/lowering input with one immutable program input,
and only then to versioned package
and evidence closure claims before restoring transitive verified admission.
The honest FAIL-bundle verifier also needs source-derived sidecar and security
context checks with typed legacy scope and a current strict check contract;
reviewed draft patches remain unintegrated. Continue the distinct
[min/max precision controls](evidence/MINMAX_REPEATABILITY_2026-09-26/README.md)
and broader value-position match controls without treating refusal of valid
code as completion. The canonical defect
status is [CLAIMS](CLAIMS.md), and the requirement dependencies remain in
[the requirement map](mission/REQUIREMENTS.md). PR #44 still requires a
reviewable split and human review before protected-branch integration.
