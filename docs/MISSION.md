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
reviewed compiler code through `aa9c33b76a9c69607a53627e61e01e0db5abcab6`,
captured-bridge receipt/log correction through `cca4941582bb081d403b89afb941907b1fe7bb04`,
fixture registrations through `49baf0bdfeb054bee41be3dd4492be0fbb13eb8d`,
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
package/evidence admission was verified. The subsequent
[solver-stream receipt](evidence/CAPTURED_SOLVER_STREAM_2026-09-26/README.md)
binds `ee4f13bb`, focused lead tests, Clippy, format and final static review.
The private captured path now returns a typed integrity failure for malformed
streams while retaining distinct wire `FAIL` and `UNKNOWN` rows. The later
[row-inventory receipt](evidence/CAPTURED_SOLVER_INVENTORY_2026-09-26/README.md)
binds `aa9c33b7` and shows that missing, copied, extra, or reordered rows
relative to the final typed inventory become compiler integrity failures.
It does not recover obligations omitted before that inventory or authenticate
the SMT encoding. Typed producer tool errors, source-to-obligation
correspondence, and a public captured-build consumer remain required before
CLI or evidence admission. Then independently review staged-publisher and CLI
units with imported contract twins;
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

The newer [capture and early-exit controls](evidence/D9_CAPTURE_EXIT_CONTROLS_2026-09-26/README.md)
are source-frozen at `49baf0bd`. A SHA-identified Safe CLI accepts both valid
twins and refuses both invalid twins through existing IFC v2. This is a
scoped baseline, not closure of the direct D9 binding/continuation producer.
Independent static reviews held both unintegrated AST binding-inventory
drafts: [v1](mission/review/D9_V7_BINDING_INVENTORY_V1_HOLD_2026-09-26.md)
could skip enclosed expressions for an empty Or pattern; [v2](mission/review/D9_V7_BINDING_INVENTORY_V2_HOLD_2026-09-26.md)
fixed that traversal but its function-local observation may disappear from a
release build. Neither patch was applied or lead-built. A smaller direct-PC
consumer is now committed at `e4bfe3001df96e2c1799710cf16532c4d57e4df7`:
the [binding-site receipt](evidence/D9_PC_BINDING_SITES_2026-09-26/README.md)
binds its compiler source, CLI, selected Safe baseline and candidate, native
positive run, focused tests, Clippy, format, and independent actual-diff plus
raw-evidence review. The two protected-shadow valid cases now pass; every
D9q negative remains refused in that scoped comparison. Nine D9q valid cases
still refuse. The next executable soundness dependency is a reviewed binding
and effect model for the remaining loop, field, call, and early-exit forms,
with a same-source negative/valid comparison and explicit unresolved
outcomes. The private captured-project typed producer/tool outcome
[v1](mission/review/CAPTURED_TOOL_OUTCOME_V1_HOLD_2026-09-26.md) patch was
held because a present but failed Z3 process could be treated as absent in
native-authoritative paths. Scratch
[v2](mission/review/CAPTURED_TOOL_OUTCOME_V2_HOLD_2026-09-26.md) fixed those
paths but was held because `unknown` followed by a tool error could still be
classified as an ordinary undecided answer or permit a native proof. A later
real-Z3 `unknown` and normal `(get-model)` unavailable response forced a
narrower classifier. The final independently reviewed process-outcome code is
committed at `2683669c2ae87ceea66d19ebb0e6995578c289eb`, with a
[source-bound local receipt](evidence/CAPTURED_TOOL_OUTCOME_2026-09-26/README.md).
The clean-HEAD CLI matches the immutable selected-check snapshot. Focused
process controls, solver/captured tests, strict Clippy, format and selected
D9q/Family-1 Safe comparisons passed; the comparisons show no raw-output
flips on their selected sources. Existing D9 valid refusals and A-EVID-1 model
replay gaps remain. No source-to-obligation or solver-encoding correspondence,
full matrix, hosted gate or release seal follows. The next evidence dependency
is a typed declaration/model inventory with checked replay; the next soundness
dependency is reviewed field/call qualifier transfer with valid controls.
Source-to-obligation checks and a public captured-build consumer remain later
dependencies; package or evidence admission must not expand in the meantime.

No current full workspace, disposable-guest, independent clean-room,
protected-branch, product-release, or full trust-chain result is claimed.
The approved main-based front-page correction is pushed and open as
[PR #48](https://github.com/AnubisQuantumCipher/anubis-lang/pull/48), head
`472160b16e9daf395540417b44f9e71c4cfc8b6e`, including the reviewed follow-up
to `90ebeaa3`. It is unmerged. Its body supersedes the alternate
[PR #45](https://github.com/AnubisQuantumCipher/anubis-lang/pull/45), which
is still open; do not merge both. The push run
[passed](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36274397254),
while the pull-request run [failed G3_test](https://github.com/AnubisQuantumCipher/anubis-lang/actions/runs/36274422605)
on an identical Git tree. The failed run did not retain its Cargo log, so
the underlying test/error is unknown. The separately approved main-based CI
diagnostic repair is pushed as draft [PR #49](https://github.com/AnubisQuantumCipher/anubis-lang/pull/49),
head `128a251753448ff8b3e62e2189b4d81324ae34c0`, with hosted checks pending
at publication. It preserves bounded failure diagnostics; it does not explain
the original G3 failure. Draft [PR #44](https://github.com/AnubisQuantumCipher/anubis-lang/pull/44)
and the [local review stack](mission/PR44_LOCAL_SPLIT_2026-09-26.md) still need
reviewable integration units and human review. Protected integration retains
its applicable authorization boundary.

Latest local code checkpoint: `da19222aed7695e6eaa3d5f4cab6d05933e27d77` on
`codex/register-round2-20260926`. The [replay inventory receipt](evidence/REPLAY_MODEL_INVENTORY_2026-09-26/README.md)
binds a reviewed test-only module; the next executable evidence dependency is
production typed value pinning and unsupported/mismatch propagation. The
[value-position if-let registration](evidence/M_IFLET_EXPR_REGISTRATION_2026-09-26/README.md)
is also banked, with negative cases undecided and the valid twin accepted.
The direct-field precision design is being amended for lexical stores,
loop fixed points and point-specific consumer invalidation before any clean
projection may suppress a legacy source. The proposed clause-based docs
scanner patch remains withheld because independent review found dropped
existing stamps. Preserve the scanner and gate floor while adding explicit
structured claim coverage. These are open dependencies, not completed gates.

The subsequent fixture-only checkpoint `59e4f1f94c4fd60d011b87eb6cb708fbcb84f4fa`
registers [direct-field loop and sibling controls](evidence/D9_DIRECT_FIELD_CONTROLS_2026-09-26/README.md).
Their immutable baseline refuses the valid projections and lacks a direct
writer-located finding on the invalid loop. The coupled production field
transfer and typed model-pinning candidates remain unintegrated pending
independent review and lead-owned verification.
