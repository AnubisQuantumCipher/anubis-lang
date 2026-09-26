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
reviewed compiler code through `365da05dbd4637323f646f95ec204d571a7ddcb1`,
captured-bridge receipt/log correction through `1ca7e5b93cf7db62ddc9392d0240c57ff1d3d728`,
fixture registrations through `e6b4fb7e86286c6314595284c0e014ca6bd49813`,
and the reviewed captured-build design at
`f7ea19097320a23fac3120359e2f226a034ee905`. The current branch has no
integrated D9 v6 or production captured-build CLI consumer code.
The [guard-transfer receipt](evidence/GUARD_WRITE_TRANSFER_2026-09-26/README.md)
binds the preceding `88a2903c` code, selected Safe checks, a clean-head
binary and a complete provisional same-source matrix comparison. Its reached
shadowed-binder call is now DISPROVED and the required valid failed-guard
fallthrough is accepted. Four valid performance forms timed out in the full
candidate run; their cause is unresolved. One OOM-designated form requires
the mandatory disposable guest for follow-up.
The historical full matrix remains `INCOMPLETE`, with other silent accepts,
valid refusals and unbound diagnostic classes.
The hosted [ordinary Linux lane](evidence/LINUX_HOSTED_2026-09-26/README.md)
has since passed on both architectures at the inspected PR #44 head
`195e33592202337e32be6ca538c45b83a75dd636`. This bounded result does
not replace a full workspace, soundness, installation, guest or release gate;
the earlier failed receipts remain historical evidence. A future split needs
new source-matched hosted witnesses.

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
The separate [no-`prove` CLI receipt](evidence/NO_DEFAULT_PROVE_CLI_2026-09-26.md)
binds `5e3f76df`: both named Clippy configurations now pass, a Safe
`run --input-json` twin works, and unsupported proof commands return explicit
errors. This does not change the evidence claim scope or supply a proof run.

The [captured Safe compilation comparison](evidence/CAPTURED_SAFE_COMPILATION_BRIDGE_2026-09-26/README.md)
now pairs its combined AST with a compilation-only source graph from one
captured input. Source-bound focused resolver tests, compiler-library Clippy,
and workspace format check pass on `339a960a`; a same-named changed-body
control confirms that lowering uses the saved import. This private API is not
used by the CLI and grants no package or evidence admission. The
[reviewed captured Safe build design](evidence/CAPTURED_SAFE_BUILD_DESIGN_REVIEW_2026-09-26.md)
led to [compiler-side code commit `365da05d` and receipt](evidence/CAPTURED_SAFE_CHECK_RESULT_2026-09-26/README.md):
pre-desugaring mode summaries now prevent unused and overridden trait
defaults from vanishing before the Safe classifier. Focused tests, compiler
Clippy and format passed; the initial parser-invalid test failure and its
correction are retained. No production CLI consumer, native publication or
package/evidence admission was verified. The next CLI dependency is a typed
solver-stream tool-error outcome, then independently reviewed staged-publisher
and CLI units with imported contract twins;
the valid transitive package still fails at `ANUBIS_DEP_PROOF_UNVERIFIED`.

The next soundness implementation dependency is a typed distinction between protected
struct fields and whole-value secrecy, carried through direct reads, calls,
aliases, returns, joins and writes. Preserve the negative controls and compare
all frozen forms on the same source-bound pins before claiming restored
precision. In parallel, connect the
[captured source graph](evidence/CAPTURED_COMBINE_ADAPTER_2026-09-26.md)
to the production CLI without extending package admission prematurely. The D9 scratch v2 patch remains
withheld after independent review found possible label laundering through
uncertified call-returned structs and stale field shapes after nested writes.
Four matched [factory and nested-write witnesses](evidence/D9_QUALIFIER_V3_REGISTRATION_2026-09-26/README.md)
are now frozen and source-bound at `fc72526b`: both REJECT controls receive
typed security refusals, the factory public sibling passes, and the valid
clean nested write is over-refused. The
[released-field rewrite pair](evidence/D9_RELEASED_FIELD_REWRITE_REGISTRATION_2026-09-26/README.md)
is also frozen in fixture-only commit `66af6419`: the valid public write/read
passes at the SHA-identified baseline, and the invalid post-release write to
declared secret `k` gets both direct and IFC2 typed refusals. Independent
review held the D9 scratch v3 patch before application because its direct
producer could lose that newly created obligation.
The further [secret-PC/public-field pair](evidence/D9_PC_PUBLIC_FIELD_REGISTRATION_2026-09-26/README.md)
is registered in fixture-only commit `15bb53a5`. The pinned baseline
over-refuses its public-guard twin and refuses the secret-guard source through
a generic direct root finding plus IFC2. Independent static review held v4
because the new field-shape shortcut could lose that direct finding without
creating a true implicit-flow obligation. The further
[D9 control-flow twins](evidence/D9_PC_PRECISION_TWINS_2026-09-26/README.md)
are frozen in `e6b4fb7e`. Their source-matched Safe baseline has protocol-valid
JSON summaries for every check and typed security diagnostics for refusals:
loop-local, WhileLet-binder and protected-shadow
valid programs are over-refused; uncalled closure, secret-bound local loop
and literal-dead controls pass; all invalid twins refuse. The reachable
`Row.n` twins still lack a writer-located PC diagnostic, so their generic
egress refusal cannot close that producer. Independent review held v5 before
application for binder, shadow, deferred-closure and reachability defects.
The subsequent [v6 independent static review](mission/review/D9_V6_HOLD_2026-09-26.md)
also held the unintegrated candidate: its callable set is incomplete, lambda
captures and active binders lack stable identity, early exits lose control
dependence, and ordinary local work can be overrefused. Its compile-time field
type mismatch and typed diagnostics mapping are recorded separately. No v6
binary or Anubis verdict exists. A revised transfer must pass independent
design and actual-diff review before a lead-only
build and same-source selected checks. The matrix
[canonical-manifest dependency](mission/REQUIREMENTS.md) remains open despite
the new fixed-path loader: production diagnostics still lack complete stable
source locators. Preserve the old `history.tsv` and add compact event history
only after that gate is reviewed. `docs/CLAIMS.md` is the defect authority.

No current full workspace, disposable-guest, independent clean-room,
protected-branch, product-release, or full trust-chain result is claimed.
A separate main-based front-page correction is now open as
[PR #45](https://github.com/AnubisQuantumCipher/anubis-lang/pull/45). The
earlier reviewed local docs-only alternative through
`90ebeaa34a38bf6286298700a71fafec76038864` remains unpushed; do not open
a duplicate. Draft [PR #44](https://github.com/AnubisQuantumCipher/anubis-lang/pull/44)
and the [local review stack](mission/PR44_LOCAL_SPLIT_2026-09-26.md) still need
reviewable integration units and human review. Protected integration retains
its applicable authorization boundary.
